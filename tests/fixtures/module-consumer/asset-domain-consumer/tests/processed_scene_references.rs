//! Public processed Scene references, data admission and native Door-driven travel.
//! Runtime bases retain strict processed data after all producer inputs are removed.
//! Source discovery and expansion of omitted Schema defaults belong to source/cook consumers.

use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetRef,
        AssetSourceChanges, AssetSourceKind, AssetSourceRoot, Assets, ProjectAssetDatabase,
    },
    core::{ByteLimit, FormatKind, FormatVersion, ItemLimit},
    ecs::{Component, Entity, Resource, World},
    fs::ContentDigest,
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{
        ImageAsset, ImageColorSpace, ImageDomain, ImageExtent, ImageFormat, ImageRuntimeCodec,
    },
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, HeadlessRunReport,
        PreparedProjectSceneView, ProjectDomainAssets, ProjectSceneRequestError,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes, RuntimeConfigurationPlan,
        advanced::commit_prepared_project_scene, open_explicit_product_project_root,
        resolve_runtime_configuration,
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogError,
        RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntitySource, SourceDocumentKey, SourceDocumentKind,
        advanced::SceneProductTransactionLimits,
    },
};
use nara_asset::{
    AssetImportSource, AssetProductRef, AssetReferenceDemand, AssetRuntimeError,
    AssetRuntimeLimits, ImportedProductId, RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
};
use nara_asset_domain_consumer::{TextBundle, TextBundleDomain, TextBundleSettings};
use nara_diagnostic::DiagnosticValueRef;

const A: u64 = 1;
const B: u64 = 2;
const C: u64 = 3;
const DOOR: &str = "consumer.processed-scene.DoorTarget";
const FIXTURE_PREFIX: &str = "nara-processed-scene-references-";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.processed-scene.DoorTarget",
    version = 1,
    alias = "Door target",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct DoorTarget {
    #[nara(
        id = "target",
        alias = "Target Scene",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    target: AssetRef,
}

nara::persistent_components! { mod components = [DoorTarget]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.processed-scene.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.processed-scene"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.processed-scene.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Resource)]
enum Case {
    Cycle,
    HiddenDependency,
    MalformedDependency,
    FailedLaterRequest,
    MissingEdge,
    SoftEdge,
    DeferredTravel,
    DeferredMalformed,
    ExtraMandatory,
    WrongDemandRole,
    MissingReference,
}

impl Case {
    fn uses_demand(self) -> bool {
        matches!(
            self,
            Self::DeferredTravel
                | Self::DeferredMalformed
                | Self::ExtraMandatory
                | Self::WrongDemandRole
                | Self::MissingReference
        )
    }
}

impl ProductConfiguration for Case {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.push(*self as u8);
    }
}

struct DoorProbePlugin(Case);

impl Plugin for DoorProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-scene"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0)?.init_resource::<Outcome>()?;
        add_startup_scene_activation_systems(app, inspect_startup)?;
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Request,
    Fault,
    Preparing,
    Ready,
    Done,
}

#[derive(Clone, Debug, Default, Resource)]
struct Outcome {
    phase: Phase,
    request: Option<ProjectSceneRequestId>,
    commits: u8,
    ready_boundaries: u8,
    failed_later: bool,
    receipts: Vec<SpawnedSceneInstance>,
}

fn inspect_startup(world: &mut World) {
    let (_, _, receipt) = live_door(world, A);
    world.resource_mut::<Outcome>().receipts.push(receipt);
}

fn travel(world: &mut World) {
    let (phase, commits) = {
        let outcome = world.resource::<Outcome>();
        (outcome.phase, outcome.commits)
    };
    if phase == Phase::Done {
        return;
    }
    let active = if commits == 0 { A } else { B };
    let (old_entity, door_target, receipt) = live_door(world, active);
    assert_eq!(world.resource::<Outcome>().receipts.last(), Some(&receipt));
    match phase {
        Phase::Request => {
            let case = *world.resource::<Case>();
            if case == Case::HiddenDependency {
                // Membership admitted B as data; this actual Door still cannot activate it.
                assert_eq!(door_target, product(B));
                assert_eq!(
                    ProjectScenes::scope(world, |scenes| scenes.request(door_target)),
                    Err(ProjectSceneRequestError::UndeclaredScene),
                );
                world.resource_mut::<Outcome>().phase = Phase::Done;
                return;
            }
            let fault =
                case == Case::FailedLaterRequest && !world.resource::<Outcome>().failed_later;
            let request = if fault {
                // C is a deliberate fault probe outside A's Required closure.
                ProjectScenes::scope(world, |scenes| scenes.request(product(C))).unwrap()
            } else {
                ProjectScenes::scope(world, |scenes| scenes.request(door_target)).unwrap()
            };
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.request = Some(request);
            outcome.phase = if fault {
                Phase::Fault
            } else {
                Phase::Preparing
            };
        }
        Phase::Fault | Phase::Preparing => {
            let request = world.resource::<Outcome>().request.unwrap();
            let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
            if state == ProjectSceneRequestState::Preparing {
                std::thread::sleep(Duration::from_millis(1));
                return;
            }
            if phase == Phase::Fault {
                assert_eq!(
                    state,
                    ProjectSceneRequestState::Rejected(ProjectSceneRequestError::RuntimeContent),
                );
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    let diagnostics = scenes.diagnostics(request).unwrap();
                    assert!(diagnostics.has_errors());
                    assert!(diagnostics.iter().all(|diagnostic| {
                        diagnostic.code().as_str() == "project.scene.runtime-content-rejected"
                    }));
                    assert!(scenes.acknowledge(request));
                });
                assert_eq!(live_door(world, A).2, receipt);
                let mut outcome = world.resource_mut::<Outcome>();
                outcome.failed_later = true;
                outcome.request = None;
                outcome.phase = Phase::Request;
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            let next = if commits == 0 { B } else { A };
            ProjectScenes::scope(world, |scenes| {
                let view = scenes.prepared(request).unwrap();
                assert_eq!(view.selected_scene_product(), Some(door_target));
                assert_eq!(view.selected_scene_path(), None);
                assert_eq!(view.expected_instance(), &receipt);
                document_target(view.scene(), next);
            });
            // End this ordinary frame with target data Ready and only the old Door in ECS.
            assert_eq!(live_door(world, active).2, receipt);
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.ready_boundaries += 1;
            outcome.phase = Phase::Ready;
        }
        Phase::Ready => {
            let request = world.resource::<Outcome>().request.unwrap();
            let report = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let published = report.instance.unwrap();
            let next = if commits == 0 { B } else { A };
            assert_eq!(live_door(world, next).2, published);
            assert_ne!(published.instance_id(), receipt.instance_id());
            assert!(world.get::<DoorTarget>(old_entity).is_none());
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed),
                );
                assert!(scenes.acknowledge(request));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.receipts.push(published);
            outcome.request = None;
            outcome.commits += 1;
            outcome.phase = if outcome.commits == 2 {
                Phase::Done
            } else {
                Phase::Request
            };
        }
        Phase::Done => unreachable!(),
    }
}

fn live_door(world: &mut World, scene: u64) -> (Entity, AssetProductRef, SpawnedSceneInstance) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    let current = CurrentScene::get(world).unwrap();
    let expected_target = document_target(current.source(), scene);
    let receipt = current.receipt().clone();
    current
        .source_view()
        .with_runtime_scene(|candidate| {
            assert_eq!(candidate.source_key(), &source_key(scene));
        })
        .expect("the current Scene retains processed data");
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &door_id(scene)) else {
        panic!("the current Scene's Door must resolve through its actual receipt")
    };
    let target = world
        .get::<DoorTarget>(entity)
        .unwrap()
        .target
        .as_product()
        .unwrap();
    assert_eq!(target, expected_target);
    assert_eq!(world.query::<&DoorTarget>().iter(world).count(), 1);
    assert_eq!(world.query::<&SceneEntitySource>().iter(world).count(), 1);
    (entity, target, receipt)
}

fn document_target(document: &SceneDocument, scene: u64) -> AssetProductRef {
    assert_eq!(document.roots, [door_id(scene)]);
    assert_eq!(document.entities.len(), 1);
    let record = &document.entities[0].components[&ComponentTypeId::new(DOOR)];
    assert_eq!(record.version, ComponentSchemaVersion::ONE);
    let door = DoorTarget::__decode_persistent_component(&record.value).unwrap();
    let target = door.target.as_product().unwrap();
    assert_eq!(target, product(if scene == A { B } else { A }));
    target
}

#[test]
fn processed_scene_cycle_travels_from_native_door_targets_without_source() {
    let base = Fixture::new(Case::Cycle).capture_and_remove();
    for scene in [A, B] {
        let data = base.acquire_scene(product(scene)).unwrap();
        assert!(std::ptr::eq(data.base().catalog(), base.catalog()));
        assert!(
            data.candidate()
                .definitions()
                .matches(base.plan().schema_validation().bindings())
        );
        document_target(data.candidate().document(), scene);
    }
    assert_round_trip(completed(execute(base)));
}

#[test]
fn processed_hidden_scene_dependency_is_data_without_public_root_authority() {
    let base = Fixture::new(Case::HiddenDependency).capture_and_remove();
    assert!(base.member(product(B)).is_some());
    assert!(!base.catalog().scenes().contains(&product(B)));
    assert!(matches!(
        base.acquire_scene(product(B)),
        Err(RuntimeContentError::UndeclaredScene)
    ));
    let outcome = completed(execute(base));
    assert_eq!(outcome.phase, Phase::Done);
    assert_eq!(outcome.commits, 0);
    assert_eq!(outcome.receipts.len(), 1);
}

#[test]
fn processed_malformed_hidden_scene_dependency_rejects_startup() {
    let base = Fixture::new(Case::MalformedDependency).capture_and_remove();
    document_target(
        base.acquire_startup_scene().unwrap().candidate().document(),
        A,
    );
    assert!(matches!(
        base.acquire_scene(product(B)),
        Err(RuntimeContentError::UndeclaredScene)
    ));
    assert_startup_rejected(base, "scene-rejected");
}

#[test]
fn processed_later_scene_failure_preserves_last_good_door_scene() {
    let base = Fixture::new(Case::FailedLaterRequest).capture_and_remove();
    assert!(base.catalog().scenes().contains(&product(C)));
    for scene in [A, B] {
        assert!(
            base.member(product(scene))
                .unwrap()
                .dependencies()
                .iter()
                .all(|edge| edge.product() != product(C))
        );
    }
    let outcome = completed(execute(base));
    assert!(outcome.failed_later);
    assert_round_trip(outcome);
}

#[test]
fn processed_missing_door_dependency_rejects_startup_without_source_fallback() {
    let base = Fixture::new(Case::MissingEdge).capture_and_remove();
    document_target(
        base.acquire_startup_scene().unwrap().candidate().document(),
        A,
    );
    assert!(matches!(
        base.acquire_scene(product(B)),
        Err(RuntimeContentError::DependencyMismatch { .. })
    ));
    assert_startup_rejected(base, "dependency-mismatch");
}

#[test]
fn processed_soft_door_dependency_rejects_startup_without_source_fallback() {
    let base = Fixture::new(Case::SoftEdge).capture_and_remove();
    document_target(
        base.acquire_startup_scene().unwrap().candidate().document(),
        A,
    );
    assert!(matches!(
        base.acquire_scene(product(B)),
        Err(RuntimeContentError::DependencyMismatch { .. })
    ));
    assert_startup_rejected(base, "dependency-mismatch");
}

fn assert_round_trip(outcome: Outcome) {
    assert_eq!(outcome.phase, Phase::Done);
    assert_eq!(outcome.commits, 2);
    assert_eq!(outcome.ready_boundaries, 2);
    assert_eq!(outcome.receipts.len(), 3);
    for pair in [(0, 1), (1, 2), (0, 2)] {
        assert_ne!(
            outcome.receipts[pair.0].instance_id(),
            outcome.receipts[pair.1].instance_id(),
        );
    }
}

fn completed(report: HeadlessRunReport<Outcome>) -> Outcome {
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics(),
    );
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("the bounded Door workflow must complete")
    };
    outcome
}

fn assert_startup_rejected(base: RuntimeContentBase, reason: &str) {
    let report = execute(base);
    assert!(matches!(report.outcome(), HeadlessRunOutcome::Failed));
    assert!(
        report.diagnostics().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "project.run.runtime-content-rejected"
                && diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "reason"
                        && field.value() == DiagnosticValueRef::Identifier(reason)
                })
        }),
        "{:?}",
        report.diagnostics(),
    );
}

fn execute(base: RuntimeContentBase) -> HeadlessRunReport<Outcome> {
    let options = HeadlessRunOptions::frames(NonZeroU32::new(20_000).unwrap(), Duration::ZERO)
        .stop_when(|outcome: &Outcome| outcome.phase == Phase::Done);
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        assert!(
            Instant::now() < deadline,
            "the processed Host must return cleanup",
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

struct Fixture {
    root: PathBuf,
    expected: RuntimeContentExpectation,
    recipe: ProductRecipe,
    demand_configuration: Option<DemandConfiguration>,
}

impl Fixture {
    fn new(case: Case) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let fixture = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_PREFIX}{}-{}",
            std::process::id(),
            fixture,
        ));
        fs::create_dir(&root).unwrap();
        if case.uses_demand() {
            return Self::demand(root, DemandConfiguration { case, fixture });
        }
        let recipe = ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<DoorProbePlugin>::configured(
                    case,
                    |case| DoorProbePlugin(*case),
                    [PROVIDER],
                )
                .unwrap(),
            )
            .unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let mut members = Vec::new();
        let scenes = if case == Case::FailedLaterRequest {
            vec![A, B, C]
        } else {
            vec![A, B]
        };
        for scene in &scenes {
            let target = if *scene == A { B } else { A };
            let mut encoded = scene_bytes(&plan, *scene, target);
            if (*scene == B && case == Case::MalformedDependency) || *scene == C {
                // Integrity is valid, but the strict Runtime entity grammar rejects this field.
                let mut wire: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
                wire["payload"]["entities"][0]["prefab"] = serde_json::Value::Null;
                encoded = serde_json::to_vec(&wire).unwrap();
            }
            // Refuse actual leaves in dependent B, even though root A's own edge is valid.
            let role = match (*scene, case) {
                (B, Case::MissingEdge) => None,
                (B, Case::SoftEdge) => Some(RuntimeContentDependencyRole::Soft),
                _ => Some(RuntimeContentDependencyRole::Required),
            };
            let path = format!("{scene}.runtime.json");
            fs::write(root.join(&path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: source_key(*scene),
                    },
                    vec![product(*scene)],
                    role.into_iter()
                        .map(|role| RuntimeContentDependency::new(product(target), role))
                        .collect(),
                )
                .unwrap(),
            );
        }
        let declared = if matches!(case, Case::HiddenDependency | Case::MalformedDependency) {
            vec![product(A)]
        } else {
            scenes.into_iter().map(product).collect()
        };
        let target = RuntimeContentTarget::new("test-host", "processed-scene-references").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: product(A),
                scenes: declared,
                addressable_roots: vec![],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
            recipe,
            demand_configuration: None,
        }
    }

    fn capture_and_remove(self) -> RuntimeContentBase {
        let base = RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap();
        self.remove_inputs();
        assert!(!self.root.exists());
        base
    }

    fn remove_inputs(&self) {
        if self.root.exists() {
            let root = self.root.canonicalize().unwrap();
            assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
            assert!(
                root.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(FIXTURE_PREFIX)
            );
            fs::remove_dir_all(root).unwrap();
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.remove_inputs();
    }
}

fn scene_bytes(plan: &RuntimeConfigurationPlan, scene: u64, target: u64) -> Vec<u8> {
    let components = BTreeMap::from([(
        ComponentTypeId::new(DOOR),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            DoorTarget {
                target: AssetRef::Product(product(target)),
            }
            .__encode_persistent_component()
            .unwrap(),
        ),
    )]);
    let id = door_id(scene);
    let source = source_key(scene);
    let wire = serde_json::json!({
        "kind": "runtime_scene", "format_version": 1, "engine_min_version": "0.1.0",
        "generator": {"name": "processed-scene-reference-test", "version": "0.0.0"},
        "payload": {
            "source_key": source,
            "roots": [id],
            "entities": [{"id": id, "children": [], "components": components}],
            "provenance": [{"entity": id, "local_entity": id, "origin": {"kind": "local"}}]
        }
    });
    RuntimeSceneDocumentCandidate::from_json_bytes(
        &serde_json::to_vec(&wire).unwrap(),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .admit(&source, plan.schema_validation().bindings())
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

fn product(scene: u64) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("73110000-0000-4000-8000-{scene:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn source_key(scene: u64) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(product(scene).source(), SourceDocumentKind::Scene)
}

fn door_id(scene: u64) -> SceneEntityId {
    SceneEntityId::new(format!("door-{scene}")).unwrap()
}

const D: u64 = 4;
const ATLAS: u64 = 10;
const HEAVY: u64 = 11;
const CPU: u64 = 12;
const CPU_ENTRY: &str = "73310000-0000-4000-8000-000000000099";
const DEMAND_DOOR: &str = "consumer.processed-scene.DeferredDoor";
const RULES: &str = "consumer.processed-scene.EncounterRules";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.processed-scene.DeferredDoor",
    version = 1,
    alias = "Deferred Door",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit, asset_ref)
)]
struct DeferredDoor {
    #[nara(id = "destinations", alias = "Destinations", asset_demand = deferred)]
    destinations: Option<Vec<AssetRef>>,
    #[nara(id = "atlas", alias = "Atlas")]
    atlas: AssetRef,
    #[nara(id = "future_atlas", alias = "Future atlas", asset_demand = deferred)]
    future_atlas: Option<Vec<AssetRef>>,
    #[nara(id = "rules", alias = "Rules")]
    rules: Option<AssetRef>,
    #[nara(id = "heavy", alias = "Heavy Image")]
    heavy: Option<AssetRef>,
    #[nara(id = "cpu", alias = "CPU Products")]
    cpu: Vec<AssetRef>,
}

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.processed-scene.EncounterRules",
    version = 1,
    alias = "Encounter rules",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct EncounterRules {
    #[nara(id = "spawn_count", alias = "Spawn count")]
    spawn_count: u32,
}

nara::persistent_components! { mod demand_components = [DeferredDoor, EncounterRules]; }

const DEMAND_PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.processed-scene.demand-schema");
const DEMAND_PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.processed-scene.demand"),
    DEMAND_PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.processed-scene.demand-native", 1),
    demand_components::initial_catalog,
    demand_components::register,
);

#[derive(Clone, Copy, Debug, Resource)]
struct DemandConfiguration {
    case: Case,
    fixture: u64,
}

impl ProductConfiguration for DemandConfiguration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        self.case.write_canonical(output);
        output.extend_from_slice(&self.fixture.to_le_bytes());
    }
}

impl DemandConfiguration {
    fn product(self, member: u64) -> AssetProductRef {
        let identity = self.fixture.checked_mul(256).unwrap() + member;
        AssetProductRef::new(
            StableAssetId::parse_str(format!("73310000-0000-4000-8000-{identity:012x}")).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap()
    }

    fn cpu_entry(self) -> AssetProductRef {
        AssetProductRef::new(
            self.product(CPU).source(),
            ImportedProductId::parse_subproduct(CPU_ENTRY).unwrap(),
        )
        .unwrap()
    }
}

// Codec callbacks execute on preparation workers. Complete fixture-specific Product identities
// isolate observations even when this test binary runs several independent Hosts concurrently.
static DEMAND_DECODES: Mutex<BTreeMap<AssetProductRef, usize>> = Mutex::new(BTreeMap::new());

fn record_demand_decode(product: AssetProductRef) {
    *DEMAND_DECODES.lock().unwrap().entry(product).or_default() += 1;
}

fn reset_demand_decodes(configuration: DemandConfiguration) {
    let mut counts = DEMAND_DECODES.lock().unwrap();
    for member in [ATLAS, HEAVY, CPU] {
        counts.insert(configuration.product(member), 0);
    }
}

fn demand_decodes(configuration: DemandConfiguration) -> [usize; 3] {
    let counts = DEMAND_DECODES.lock().unwrap();
    [ATLAS, HEAVY, CPU].map(|member| {
        counts
            .get(&configuration.product(member))
            .copied()
            .unwrap_or(0)
    })
}

struct DemandImageCodec;

impl RuntimeAssetCodec<ImageAsset> for DemandImageCodec {
    const RUNTIME_KIND: &'static str = ImageRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = ImageRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        ImageRuntimeCodec::runtime_limits()
    }

    fn encode_runtime(image: &ImageAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        ImageRuntimeCodec::encode_runtime(image)
    }

    fn decode_runtime(encoded: &[u8]) -> Result<ImageAsset, AssetRuntimeError> {
        let image = ImageRuntimeCodec::decode_runtime(encoded)?;
        record_demand_decode(image.product().unwrap());
        Ok(image)
    }

    fn runtime_retained_bytes(image: &ImageAsset) -> Result<usize, AssetRuntimeError> {
        ImageRuntimeCodec::runtime_retained_bytes(image)
    }

    fn visit_runtime_dependencies<E>(
        image: &ImageAsset,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        ImageRuntimeCodec::visit_runtime_dependencies(image, visitor)
    }

    fn contains_runtime_product(image: &ImageAsset, reference: AssetProductRef) -> bool {
        ImageRuntimeCodec::contains_runtime_product(image, reference)
    }
}

struct DemandBundleCodec;

impl RuntimeAssetCodec<TextBundle> for DemandBundleCodec {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TextBundleDomain::runtime_limits()
    }

    fn encode_runtime(bundle: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(bundle)
    }

    fn decode_runtime(encoded: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        let bundle = TextBundleDomain::decode_runtime(encoded)?;
        record_demand_decode(
            AssetProductRef::new(bundle.source(), ImportedProductId::PRIMARY).unwrap(),
        );
        Ok(bundle)
    }

    fn runtime_retained_bytes(bundle: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(bundle)
    }

    fn visit_runtime_dependencies<E>(
        bundle: &TextBundle,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_runtime_dependencies(bundle, visitor)
    }

    fn contains_runtime_product(bundle: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(bundle, reference)
    }
}

impl Fixture {
    fn demand(root: PathBuf, configuration: DemandConfiguration) -> Self {
        let recipe = ProductRecipe::new()
            .add_contribution(
                SchemaContribution::<DemandProbePlugin>::configured(
                    configuration,
                    |configuration| DemandProbePlugin(*configuration),
                    [DEMAND_PROVIDER],
                )
                .unwrap(),
            )
            .unwrap()
            .add_runtime_image_codec::<DemandImageCodec>(RuntimeCodecDefinitionId::new(
                "consumer.processed-scene.image",
                1,
            ))
            .unwrap()
            .add_runtime_asset_codec::<TextBundleDomain, DemandBundleCodec>(
                RuntimeCodecDefinitionId::new("consumer.processed-scene.text", 1),
            )
            .unwrap();
        let mut runtime = RuntimeConfiguration::default();
        runtime.runtime.preset = RuntimePreset::LocalHeadless;
        runtime.capabilities.requested = vec![ProductCapability::Runtime2d];
        runtime.tasks.io.workers = 1;
        runtime.tasks.compute.workers = 1;
        runtime.tasks.async_compute.workers = 1;
        let plan =
            resolve_runtime_configuration(&runtime.resolve().unwrap(), recipe.clone()).unwrap();
        let mut members = Vec::new();
        for scene in [A, B, C, D] {
            let mut encoded = demand_scene_bytes(&plan, configuration, scene);
            if scene == B && configuration.case == Case::DeferredMalformed {
                let mut wire: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
                wire["payload"]["entities"][0]["prefab"] = serde_json::Value::Null;
                encoded = serde_json::to_vec(&wire).unwrap();
            }
            let mut dependencies = Vec::new();
            if scene != D {
                dependencies.push(RuntimeContentDependency::new(
                    configuration.product(ATLAS),
                    RuntimeContentDependencyRole::Required,
                ));
            }
            if scene == A {
                dependencies.push(RuntimeContentDependency::new(
                    configuration.product(D),
                    RuntimeContentDependencyRole::Required,
                ));
                if configuration.case != Case::MissingReference {
                    dependencies.push(RuntimeContentDependency::new(
                        configuration.product(B),
                        if configuration.case == Case::WrongDemandRole {
                            RuntimeContentDependencyRole::Required
                        } else {
                            RuntimeContentDependencyRole::Reference
                        },
                    ));
                }
                if configuration.case == Case::ExtraMandatory {
                    dependencies.push(RuntimeContentDependency::new(
                        configuration.product(HEAVY),
                        RuntimeContentDependencyRole::Required,
                    ));
                }
            }
            if scene == B {
                dependencies.push(RuntimeContentDependency::new(
                    configuration.product(C),
                    RuntimeContentDependencyRole::Reference,
                ));
                for reference in [
                    configuration.product(HEAVY),
                    configuration.product(CPU),
                    configuration.cpu_entry(),
                ] {
                    dependencies.push(RuntimeContentDependency::new(
                        reference,
                        RuntimeContentDependencyRole::Required,
                    ));
                }
            }
            let path = format!("{scene}.runtime.json");
            fs::write(root.join(&path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: demand_source_key(configuration, scene),
                    },
                    vec![configuration.product(scene)],
                    dependencies,
                )
                .unwrap(),
            );
        }
        for (member, side, color) in [(ATLAS, 2, 37), (HEAVY, 256, 173)] {
            let image = ImageAsset::from_product(
                configuration.product(member),
                ImageExtent::new(side, side),
                ImageFormat::Rgba8,
                ImageColorSpace::Srgb,
                vec![color; side as usize * side as usize * 4],
            )
            .unwrap();
            let encoded = DemandImageCodec::encode_runtime(&image).unwrap();
            let path = format!("{member}.runtime.image");
            fs::write(root.join(&path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(DemandImageCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![configuration.product(member)],
                    vec![],
                )
                .unwrap(),
            );
        }
        let bundle = demand_import_bundle(&root, configuration);
        let encoded = DemandBundleCodec::encode_runtime(&bundle).unwrap();
        fs::write(root.join("cpu.runtime.json"), &encoded).unwrap();
        members.push(
            RuntimeContentMember::new(
                "cpu.runtime.json",
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(DemandBundleCodec::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::ONE,
                },
                vec![configuration.product(CPU), configuration.cpu_entry()],
                vec![],
            )
            .unwrap(),
        );
        let target = RuntimeContentTarget::new("test-host", "processed-scene-demand").unwrap();
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration: runtime,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: configuration.product(A),
                scenes: [A, B, C].map(|scene| configuration.product(scene)).to_vec(),
                addressable_roots: vec![],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
            recipe,
            demand_configuration: Some(configuration),
        }
    }
}

fn demand_import_bundle(root: &Path, configuration: DemandConfiguration) -> TextBundle {
    let raw = serde_json::to_vec(&serde_json::json!({
        "suffix": "suffix.txt",
        "entries": [{"name": "Encounter", "text": "B-only CPU"}],
    }))
    .unwrap();
    fs::write(root.join("input.bundle.json"), &raw).unwrap();
    fs::write(root.join("suffix.txt"), b" prepared").unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            configuration.product(CPU).source(),
            AssetPath::new("input.bundle.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        TextBundleSettings::initial(&raw, 64, [configuration.cpu_entry().product()]).unwrap(),
    )
    .to_json_string()
    .unwrap();
    fs::write(root.join("input.bundle.json.meta"), &metadata).unwrap();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(metadata.as_bytes()).unwrap();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        let bytes = fs::read(root.join(path.as_str()))?;
        assert!(bytes.len() <= limit.get());
        Ok::<Arc<[u8]>, std::io::Error>(Arc::from(bytes))
    };
    TextBundleDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        Arc::from(raw),
        &mut reader,
    ))
    .unwrap()
}

fn demand_source_key(configuration: DemandConfiguration, scene: u64) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(
        configuration.product(scene).source(),
        SourceDocumentKind::Scene,
    )
}

fn demand_scene_bytes(
    plan: &RuntimeConfigurationPlan,
    configuration: DemandConfiguration,
    scene: u64,
) -> Vec<u8> {
    let (component, value) = if scene == D {
        (
            RULES,
            EncounterRules { spawn_count: 17 }
                .__encode_persistent_component()
                .unwrap(),
        )
    } else {
        let destination = match scene {
            A => Some(vec![AssetRef::Product(configuration.product(B))]),
            B => Some(vec![AssetRef::Product(configuration.product(C))]),
            C => None,
            _ => unreachable!(),
        };
        let door = DeferredDoor {
            destinations: destination,
            atlas: AssetRef::Product(configuration.product(ATLAS)),
            future_atlas: Some(if scene == C {
                vec![]
            } else {
                vec![
                    AssetRef::Product(configuration.product(ATLAS)),
                    AssetRef::Product(configuration.product(ATLAS)),
                ]
            }),
            rules: (scene == A).then(|| AssetRef::Product(configuration.product(D))),
            heavy: (scene == B).then(|| AssetRef::Product(configuration.product(HEAVY))),
            cpu: if scene == B {
                vec![
                    AssetRef::Product(configuration.product(CPU)),
                    AssetRef::Product(configuration.cpu_entry()),
                ]
            } else {
                vec![]
            },
        };
        (DEMAND_DOOR, door.__encode_persistent_component().unwrap())
    };
    let id = door_id(scene);
    let source = demand_source_key(configuration, scene);
    let components = BTreeMap::from([(
        ComponentTypeId::new(component),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )]);
    let wire = serde_json::json!({
        "kind": "runtime_scene", "format_version": 1, "engine_min_version": "0.1.0",
        "generator": {"name": "processed-scene-demand-test", "version": "0.0.0"},
        "payload": {
            "source_key": source,
            "roots": [id],
            "entities": [{"id": id, "children": [], "components": components}],
            "provenance": [{"entity": id, "local_entity": id, "origin": {"kind": "local"}}]
        }
    });
    RuntimeSceneDocumentCandidate::from_json_bytes(
        &serde_json::to_vec(&wire).unwrap(),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .admit(&source, plan.schema_validation().bindings())
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

struct DemandProbePlugin(DemandConfiguration);

impl Plugin for DemandProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-scene.demand"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[DEMAND_PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0)?
            .init_resource::<DemandOutcome>()?;
        add_startup_scene_activation_systems(app, inspect_demand_startup)?;
        app.add_systems(CoreStage::Update, demand_travel)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DemandPhase {
    #[default]
    Request,
    Preparing,
    Ready,
    Cancelling,
    Done,
}

#[derive(Clone, Debug, Default, Resource)]
struct DemandOutcome {
    phase: DemandPhase,
    // Inspect/cancel A, cancel B, commit B, cancel C, commit C.
    leg: u8,
    request: Option<ProjectSceneRequestId>,
    commits: u8,
    cancellations: u8,
    ready: Vec<u64>,
    receipts: Vec<SpawnedSceneInstance>,
    prepared_pixels: [usize; 2],
    prepared_spawn_count: Option<u32>,
    rejected_b: bool,
}

fn demand_products(
    assets: &ProjectDomainAssets,
    configuration: DemandConfiguration,
    scene: u64,
) -> [usize; 2] {
    let images = assets.collection::<ImageDomain>().unwrap();
    assert_eq!(images.source_records().len(), 0);
    assert_eq!(images.iter().len(), if scene == B { 2 } else { 1 });
    let atlas = images
        .get(&AssetRef::Product(configuration.product(ATLAS)))
        .unwrap();
    assert_eq!(atlas.extent(), ImageExtent::new(2, 2));
    let heavy = images.get(&AssetRef::Product(configuration.product(HEAVY)));
    assert_eq!(heavy.is_some(), scene == B);
    if let Some(heavy) = heavy {
        assert_eq!(heavy.extent(), ImageExtent::new(256, 256));
    }
    let texts = assets.collection::<TextBundleDomain>();
    if scene == B {
        let texts = texts.unwrap();
        assert_eq!(texts.source_records().len(), 0);
        assert_eq!(texts.iter().len(), 1);
        let primary = texts
            .get(&AssetRef::Product(configuration.product(CPU)))
            .unwrap();
        let auxiliary = texts
            .get(&AssetRef::Product(configuration.cpu_entry()))
            .unwrap();
        assert!(std::ptr::eq(primary, auxiliary));
        assert_eq!(
            primary.get(&configuration.cpu_entry()).unwrap().text,
            "B-only CPU prepared"
        );
    } else {
        assert!(texts.is_none());
    }
    [
        atlas.pixels().as_ptr() as usize,
        heavy.map_or(0, |image| image.pixels().as_ptr() as usize),
    ]
}

fn demand_residency(world: &World, configuration: DemandConfiguration, scene: u64) -> [usize; 2] {
    let assets = world.resource::<ProjectDomainAssets>();
    let pointers = demand_products(assets, configuration, scene);
    let native = world.resource::<Assets<ImageAsset>>();
    assert_eq!(native.len(), if scene == B { 2 } else { 1 });
    let typed = assets.collection::<ImageDomain>().unwrap();
    for (_, image) in native.iter() {
        let product = image.product().unwrap();
        assert!(
            product == configuration.product(ATLAS)
                || (scene == B && product == configuration.product(HEAVY))
        );
        // Native publication must retain the typed acquisition's pixels, not decode or copy again.
        assert_eq!(
            image.pixels().as_ptr(),
            typed
                .get(&AssetRef::Product(product))
                .unwrap()
                .pixels()
                .as_ptr()
        );
    }
    pointers
}

fn live_demand_door(
    world: &mut World,
    configuration: DemandConfiguration,
    scene: u64,
) -> (Entity, Option<AssetProductRef>, SpawnedSceneInstance) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    let current = CurrentScene::get(world).unwrap();
    let receipt = current.receipt().clone();
    current
        .source_view()
        .with_runtime_scene(|candidate| {
            assert_eq!(
                candidate.source_key(),
                &demand_source_key(configuration, scene)
            );
        })
        .unwrap();
    assert_eq!(current.source().roots, [door_id(scene)]);
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &door_id(scene)) else {
        panic!("the Deferred Door must resolve through the current receipt")
    };
    let door = world.get::<DeferredDoor>(entity).unwrap();
    assert_eq!(door.atlas.as_product(), Some(configuration.product(ATLAS)));
    let future_atlas = door.future_atlas.as_ref().unwrap();
    assert_eq!(future_atlas.len(), if scene == C { 0 } else { 2 });
    assert!(
        future_atlas
            .iter()
            .all(|reference| { reference.as_product() == Some(configuration.product(ATLAS)) })
    );
    let target = door.destinations.as_ref().map(|targets| {
        assert_eq!(targets.len(), 1);
        targets[0].as_product().unwrap()
    });
    let expected = match scene {
        A => Some(configuration.product(B)),
        B => Some(configuration.product(C)),
        C => None,
        _ => unreachable!(),
    };
    assert_eq!(target, expected);
    assert_eq!(world.query::<&DeferredDoor>().iter(world).count(), 1);
    assert_eq!(world.query::<&EncounterRules>().iter(world).count(), 0);
    assert_eq!(world.query::<&SceneEntitySource>().iter(world).count(), 1);
    demand_residency(world, configuration, scene);
    (entity, target, receipt)
}

fn inspect_demand_startup(world: &mut World) {
    let configuration = *world.resource::<DemandConfiguration>();
    let (_, _, receipt) = live_demand_door(world, configuration, A);
    assert_eq!(demand_decodes(configuration), [1, 0, 0]);
    assert_eq!(
        ProjectScenes::scope(world, |scenes| scenes.request(configuration.product(D))),
        Err(ProjectSceneRequestError::UndeclaredScene),
    );
    world.resource_mut::<DemandOutcome>().receipts.push(receipt);
}

fn inspect_demand_prepared(
    view: PreparedProjectSceneView<'_>,
    configuration: DemandConfiguration,
    scene: u64,
    expected: &SpawnedSceneInstance,
) -> ([usize; 2], Option<u32>) {
    assert_eq!(
        view.selected_scene_product(),
        Some(configuration.product(scene))
    );
    assert_eq!(view.selected_scene_path(), None);
    assert_eq!(view.expected_instance(), expected);
    assert_eq!(view.scene().roots, [door_id(scene)]);
    let rules = if scene == A {
        let document = view.scene_dependency(configuration.product(D)).unwrap();
        assert_eq!(document.roots, [door_id(D)]);
        assert_eq!(document.entities.len(), 1);
        let record = &document.entities[0].components[&ComponentTypeId::new(RULES)];
        let owned = EncounterRules::__decode_persistent_component(&record.value).unwrap();
        assert_eq!(owned.spawn_count, 17);
        Some(owned.spawn_count)
    } else {
        assert!(view.scene_dependency(configuration.product(D)).is_none());
        None
    };
    // Deferred membership never exposes B/C's documents as this request's prepared CPU data.
    for deferred in [B, C] {
        assert!(
            view.scene_dependency(configuration.product(deferred))
                .is_none()
        );
    }
    (
        demand_products(view.domain_assets(), configuration, scene),
        rules,
    )
}

fn demand_travel(world: &mut World) {
    let configuration = *world.resource::<DemandConfiguration>();
    let (phase, leg, commits) = {
        let outcome = world.resource::<DemandOutcome>();
        (outcome.phase, outcome.leg, outcome.commits)
    };
    if phase == DemandPhase::Done {
        return;
    }
    let active = if commits == 0 { A } else { B };
    let (old_entity, door_target, receipt) = live_demand_door(world, configuration, active);
    assert_eq!(
        world.resource::<DemandOutcome>().receipts.last(),
        Some(&receipt)
    );
    let target = match leg {
        0 => A,
        1 | 2 => B,
        3 | 4 => C,
        _ => unreachable!(),
    };
    match phase {
        DemandPhase::Request => {
            let selected = if leg == 0 {
                configuration.product(A)
            } else {
                door_target.unwrap()
            };
            assert_eq!(selected, configuration.product(target));
            let request = ProjectScenes::scope(world, |scenes| scenes.request(selected)).unwrap();
            let mut outcome = world.resource_mut::<DemandOutcome>();
            outcome.request = Some(request);
            outcome.phase = DemandPhase::Preparing;
        }
        DemandPhase::Preparing => {
            let request = world.resource::<DemandOutcome>().request.unwrap();
            let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
            if state == ProjectSceneRequestState::Preparing {
                std::thread::sleep(Duration::from_millis(1));
                return;
            }
            if configuration.case == Case::DeferredMalformed && target == B {
                assert_eq!(
                    state,
                    ProjectSceneRequestState::Rejected(ProjectSceneRequestError::RuntimeContent)
                );
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    let diagnostics = scenes.diagnostics(request).unwrap();
                    assert!(diagnostics.has_errors());
                    assert!(diagnostics.iter().any(|diagnostic| {
                        diagnostic.code().as_str() == "project.scene.runtime-content-rejected"
                    }));
                    assert!(scenes.acknowledge(request));
                });
                assert_eq!(live_demand_door(world, configuration, A).2, receipt);
                assert_eq!(demand_decodes(configuration), [2, 0, 0]);
                let mut outcome = world.resource_mut::<DemandOutcome>();
                outcome.request = None;
                outcome.rejected_b = true;
                outcome.phase = DemandPhase::Done;
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            let (pixels, rules) = ProjectScenes::scope(world, |scenes| {
                inspect_demand_prepared(
                    scenes.prepared(request).unwrap(),
                    configuration,
                    target,
                    &receipt,
                )
            });
            let expected_decodes = match leg {
                0 => [2, 0, 0],
                1 => [3, 1, 1],
                2 => [4, 2, 2],
                3 => [5, 2, 2],
                4 => [6, 2, 2],
                _ => unreachable!(),
            };
            assert_eq!(demand_decodes(configuration), expected_decodes);
            assert_eq!(live_demand_door(world, configuration, active).2, receipt);
            let mut outcome = world.resource_mut::<DemandOutcome>();
            outcome.prepared_pixels = pixels;
            if let Some(rules) = rules {
                outcome.prepared_spawn_count = Some(rules);
            }
            outcome.ready.push(target);
            outcome.phase = DemandPhase::Ready;
        }
        DemandPhase::Ready if matches!(leg, 0 | 1 | 3) => {
            let request = world.resource::<DemandOutcome>().request.unwrap();
            ProjectScenes::scope(world, |scenes| assert!(scenes.cancel(request)));
            assert_eq!(live_demand_door(world, configuration, active).2, receipt);
            world.resource_mut::<DemandOutcome>().phase = DemandPhase::Cancelling;
        }
        DemandPhase::Cancelling => {
            let request = world.resource::<DemandOutcome>().request.unwrap();
            let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
            if state == ProjectSceneRequestState::Cancelling {
                std::thread::sleep(Duration::from_millis(1));
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Cancelled);
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.prepared(request).is_none());
                assert!(scenes.acknowledge(request));
            });
            assert_eq!(live_demand_door(world, configuration, active).2, receipt);
            let mut outcome = world.resource_mut::<DemandOutcome>();
            outcome.request = None;
            outcome.cancellations += 1;
            outcome.leg += 1;
            outcome.phase = DemandPhase::Request;
        }
        DemandPhase::Ready => {
            let request = world.resource::<DemandOutcome>().request.unwrap();
            let pixels = world.resource::<DemandOutcome>().prepared_pixels;
            let before = demand_decodes(configuration);
            let report = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let published = report.instance.unwrap();
            assert_eq!(live_demand_door(world, configuration, target).2, published);
            assert_ne!(published.instance_id(), receipt.instance_id());
            assert!(world.get::<DeferredDoor>(old_entity).is_none());
            assert_eq!(demand_residency(world, configuration, target), pixels);
            assert_eq!(demand_decodes(configuration), before);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(scenes.acknowledge(request));
            });
            let mut outcome = world.resource_mut::<DemandOutcome>();
            outcome.receipts.push(published);
            outcome.request = None;
            outcome.commits += 1;
            outcome.leg += 1;
            outcome.phase = if target == C {
                DemandPhase::Done
            } else {
                DemandPhase::Request
            };
        }
        DemandPhase::Done => unreachable!(),
    }
}

fn execute_demand(base: RuntimeContentBase) -> HeadlessRunReport<DemandOutcome> {
    let options = HeadlessRunOptions::frames(NonZeroU32::new(20_000).unwrap(), Duration::ZERO)
        .stop_when(|outcome: &DemandOutcome| outcome.phase == DemandPhase::Done);
    let mut run = HeadlessRun::<DemandOutcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            return report;
        }
        assert!(
            Instant::now() < deadline,
            "the CP3 Host must return cleanup"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn completed_demand(report: HeadlessRunReport<DemandOutcome>) -> DemandOutcome {
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("the bounded CP3 Door workflow must complete")
    };
    assert_eq!(outcome.phase, DemandPhase::Done);
    outcome
}

#[test]
fn processed_deferred_doors_validate_delivery_then_prepare_only_current_demand() {
    let fixture = Fixture::new(Case::DeferredTravel);
    let configuration = fixture.demand_configuration.unwrap();
    let base = fixture.capture_and_remove();
    assert!(base.validate_deep().is_ok());
    assert!(
        demand_decodes(configuration)
            .into_iter()
            .all(|count| count > 0)
    );
    reset_demand_decodes(configuration);
    assert!(base.member(configuration.product(D)).is_some());
    assert!(matches!(
        base.acquire_scene(configuration.product(D)),
        Err(RuntimeContentError::UndeclaredScene)
    ));
    let atlas = base
        .member(configuration.product(A))
        .unwrap()
        .dependencies()
        .iter()
        .find(|edge| edge.product() == configuration.product(ATLAS))
        .unwrap();
    assert_eq!(atlas.role(), RuntimeContentDependencyRole::Required);
    let outcome = completed_demand(execute_demand(base));
    assert_eq!(outcome.ready, [A, B, B, C, C]);
    assert_eq!(outcome.cancellations, 3);
    assert_eq!(outcome.commits, 2);
    assert_eq!(outcome.prepared_spawn_count, Some(17));
    assert!(!outcome.rejected_b);
    assert_eq!(outcome.receipts.len(), 3);
    for pair in [(0, 1), (1, 2), (0, 2)] {
        assert_ne!(
            outcome.receipts[pair.0].instance_id(),
            outcome.receipts[pair.1].instance_id()
        );
    }
    assert_eq!(demand_decodes(configuration), [6, 2, 2]);
}

#[test]
fn processed_deferred_bad_payload_fails_deep_validation_and_request_but_preserves_a() {
    let fixture = Fixture::new(Case::DeferredMalformed);
    let configuration = fixture.demand_configuration.unwrap();
    let base = fixture.capture_and_remove();
    assert!(base.validate_deep().is_err());
    reset_demand_decodes(configuration);
    let outcome = completed_demand(execute_demand(base));
    assert!(outcome.rejected_b);
    assert_eq!(outcome.ready, [A]);
    assert_eq!(outcome.cancellations, 1);
    assert_eq!(outcome.commits, 0);
    assert_eq!(outcome.receipts.len(), 1);
    assert_eq!(outcome.prepared_spawn_count, Some(17));
    assert_eq!(demand_decodes(configuration), [2, 0, 0]);
}

#[test]
fn processed_actual_demand_rejects_extra_wrong_role_and_missing_reference_before_child_decode() {
    for case in [
        Case::ExtraMandatory,
        Case::WrongDemandRole,
        Case::MissingReference,
    ] {
        let fixture = Fixture::new(case);
        let configuration = fixture.demand_configuration.unwrap();
        let base = fixture.capture_and_remove();
        assert!(base.validate_deep().is_err());
        // A is the first canonical Scene root. Rejecting its dependency inventory must stop
        // deep validation before following its children or proceeding to independent B/C roots.
        assert_eq!(demand_decodes(configuration), [0, 0, 0]);
        reset_demand_decodes(configuration);
        let (expected_product, expected_declared, expected_actual) = match case {
            Case::ExtraMandatory => (
                configuration.product(HEAVY),
                Some(RuntimeContentDependencyRole::Required),
                None,
            ),
            Case::WrongDemandRole => (
                configuration.product(B),
                Some(RuntimeContentDependencyRole::Required),
                Some(AssetReferenceDemand::Deferred),
            ),
            Case::MissingReference => (
                configuration.product(B),
                None,
                Some(AssetReferenceDemand::Deferred),
            ),
            _ => unreachable!(),
        };
        assert!(matches!(
            base.acquire_startup_scene(),
            Err(RuntimeContentError::DependencyMismatch { product, declared, actual })
                if product == expected_product
                    && declared == expected_declared
                    && actual == expected_actual
        ));
        assert_eq!(demand_decodes(configuration), [0, 0, 0]);
        let report = execute_demand(base);
        assert!(matches!(report.outcome(), HeadlessRunOutcome::Failed));
        assert!(
            report.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.run.runtime-content-rejected"
                    && diagnostic.fields().iter().any(|field| {
                        field.key().as_str() == "reason"
                            && field.value()
                                == DiagnosticValueRef::Identifier("dependency-mismatch")
                    })
            }),
            "{:?}",
            report.diagnostics()
        );
        assert_eq!(demand_decodes(configuration), [0, 0, 0]);
    }
}

#[test]
fn processed_reference_role_requires_actual_catalog_membership() {
    let fixture = Fixture::new(Case::DeferredTravel);
    let configuration = fixture.demand_configuration.unwrap();
    let base = fixture.capture_and_remove();
    let catalog = base.catalog();
    // Keep A's real Reference declaration, removing B's member and its independent travel root.
    let result = RuntimeContentCatalog::new(
        RuntimeContentCatalogDefinition {
            target: catalog.target().clone(),
            configuration: catalog.configuration().clone(),
            plugin_plan_fingerprint: catalog.plugin_plan_fingerprint(),
            schema_fingerprint: catalog.schema_fingerprint(),
            runtime_codecs_fingerprint: catalog.runtime_codecs_fingerprint(),
            startup_scene: catalog.startup_scene(),
            scenes: catalog
                .scenes()
                .iter()
                .copied()
                .filter(|product| *product != configuration.product(B))
                .collect(),
            addressable_roots: catalog.addressable_roots().to_vec(),
            members: catalog
                .members()
                .iter()
                .filter(|member| !member.products().contains(&configuration.product(B)))
                .cloned()
                .collect(),
        },
        RuntimeContentCatalogLimits::default(),
    );
    assert!(matches!(
        result,
        Err(RuntimeContentCatalogError::MissingDependencyProduct { product, .. })
            if product == configuration.product(B)
    ));
    assert_eq!(demand_decodes(configuration), [0, 0, 0]);
}
