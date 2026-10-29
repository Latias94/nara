//! Public processed Scene references, data admission and native Door-driven travel.
//! Fixtures contain only strict Runtime data; source Scene discovery is a separate workflow.

use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration, ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{AssetRef, AssetSourceChanges, AssetSourceRoot, ProjectAssetDatabase},
    core::ItemLimit,
    ecs::{Component, Entity, Resource, World},
    fs::ContentDigest,
    identity::{EntityLookup, SpawnedSceneInstance},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, HeadlessRunReport,
        ProjectSceneRequestError, ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        RuntimeConfigurationPlan, advanced::commit_prepared_project_scene,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentBase, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument,
        SceneEntityId, SceneEntitySource, SourceDocumentKey, SourceDocumentKind,
        advanced::SceneProductTransactionLimits,
    },
};
use nara_asset::{AssetProductRef, ImportedProductId, StableAssetId};
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
        Err(RuntimeContentError::UndeclaredDependency)
    ));
    assert_startup_rejected(base, "undeclared-dependency");
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
        Err(RuntimeContentError::UndeclaredDependency)
    ));
    assert_startup_rejected(base, "undeclared-dependency");
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
}

impl Fixture {
    fn new(case: Case) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_PREFIX}{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
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
