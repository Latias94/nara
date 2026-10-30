//! Public first-party processed Host startup, compound CPU reads and transactional Scene travel.
//! The producer uses the independent text importer/codec; the executable selects Runtime codecs
//! only. Its entire input directory is removed before the first managed frame.

use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{
        AssetDependencyGraph, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
        AssetRecord, AssetRef, AssetReloadRequests, AssetSourceChanges, AssetSourceKind,
        AssetSourceRoot, ProjectAssetDatabase,
    },
    core::{ByteLimit, FormatKind, FormatVersion, ItemLimit},
    ecs::{Component, Resource, World},
    fs::ContentDigest,
    identity::{EntityLookup, SpawnedSceneInstance},
    project::{ProductCapability, ProjectPath, RuntimeConfiguration, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, PreparedProjectSceneView,
        ProjectAssetCollection, ProjectDomainAssets, ProjectSceneRequestError,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectSceneSelection, ProjectScenes,
        RuntimeConfigurationPlan, advanced::commit_prepared_project_scene,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    scene::{
        ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord,
        SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
        advanced::{SceneProductResource, SceneProductTransactionLimits},
    },
};
use nara_asset::{
    AssetImportSource, AssetProductRef, AssetRuntimeError, AssetRuntimeLimits, ImportedProductId,
    RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
};
use nara_asset_domain_consumer::{TextBundle, TextBundleDomain, TextBundleSettings};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};

const STARTUP: &str = "72110000-0000-4000-8000-000000000001";
const LATER: &str = "72110000-0000-4000-8000-000000000002";
const BUNDLE_A: &str = "72110000-0000-4000-8000-000000000003";
const BUNDLE_B: &str = "72110000-0000-4000-8000-000000000004";
const BUNDLE_C: &str = "72110000-0000-4000-8000-000000000005";
const ABSENT: &str = "72110000-0000-4000-8000-000000000099";
const FIRST: &str = "72110000-0000-4000-8000-000000000011";
const SECOND: &str = "72110000-0000-4000-8000-000000000012";
const CAPTION: &str = "consumer.processed-host.Caption";

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.processed-host.Caption",
    version = 1,
    alias = "Processed Host Caption",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Caption {
    #[nara(
        id = "first",
        alias = "First",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    first: AssetRef,
    #[nara(
        id = "second",
        alias = "Second",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    second: AssetRef,
    #[nara(
        id = "subtitle",
        alias = "Subtitle",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    subtitle: AssetRef,
}

nara::persistent_components! { mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.processed-host.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.processed-host"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.processed-host.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct HostProbePlugin;

impl Plugin for HostProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-host"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Outcome>()?
            .init_resource::<CommitMarker>()?;
        add_startup_scene_activation_systems(app, inspect_startup)?;
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    // Selecting the Runtime facet of this source-capable domain never selects its importer.
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<HostProbePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_runtime_asset_codec::<TextBundleDomain, ObservedCodec<false>>(
            RuntimeCodecDefinitionId::new("consumer.processed-host.text-bundle", 1),
        )
        .unwrap()
        .add_runtime_asset_codec::<TextBundleDomain, ObservedCodec<true>>(
            RuntimeCodecDefinitionId::new("consumer.processed-host.alternate-text-bundle", 1),
        )
        .unwrap()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Startup,
    AcquisitionFailure,
    HeldWorker,
    Cancelling,
    RejectedCommit,
    PrepareCommit,
    HoldReady,
    Done,
}

#[derive(Clone, Debug, Default, Resource)]
struct Outcome {
    stage: Stage,
    request: Option<ProjectSceneRequestId>,
    startup_receipt: Option<SpawnedSceneInstance>,
    committed_receipt: Option<SpawnedSceneInstance>,
    startup_allocation: usize,
    prepared_allocations: [usize; 2],
    acquisition_rejected: bool,
    cancellation_returned: bool,
    commit_rejected: bool,
    source_free_frames: usize,
}

#[derive(Resource)]
struct CommitMarker {
    level: u32,
    incarnation: Arc<()>,
}

impl Default for CommitMarker {
    fn default() -> Self {
        Self::new(0)
    }
}

impl CommitMarker {
    fn new(level: u32) -> Self {
        Self {
            level,
            incarnation: Arc::new(()),
        }
    }
}

impl SceneProductResource for CommitMarker {
    type Baseline = (u32, Arc<()>);

    fn capture_baseline(&self) -> Self::Baseline {
        (self.level, Arc::clone(&self.incarnation))
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        self.level == baseline.0 && Arc::ptr_eq(&self.incarnation, &baseline.1)
    }
}

#[derive(Component)]
struct RuntimeTag;

fn inspect_startup(world: &mut World) {
    assert_source_free(world);
    let (receipt, allocations) = inspect_live(world, false);
    assert_counts([1, 0, 0]);
    let mut outcome = world.resource_mut::<Outcome>();
    outcome.startup_receipt = Some(receipt);
    outcome.startup_allocation = allocations[0];
}

fn travel(world: &mut World) {
    assert_source_free(world);
    world.resource_mut::<Outcome>().source_free_frames += 1;
    let stage = world.resource::<Outcome>().stage;
    if stage == Stage::Done {
        return;
    }
    assert_last_good(world);
    match stage {
        Stage::Startup => {
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.request(ProjectPath::new("later.scene.json").unwrap()),
                    Err(ProjectSceneRequestError::UndeclaredScene),
                );
                assert_eq!(
                    scenes.request(primary(ABSENT)),
                    Err(ProjectSceneRequestError::UndeclaredScene),
                );
            });
            assert_counts([1, 0, 0]);
            arm(DecodeAction::Reject);
            request_later(world, Stage::AcquisitionFailure);
        }
        Stage::AcquisitionFailure => {
            let request = pending(world);
            let state = request_state(world, request);
            if state == ProjectSceneRequestState::Preparing {
                yield_worker();
                return;
            }
            assert_eq!(
                state,
                ProjectSceneRequestState::Rejected(ProjectSceneRequestError::RuntimeContent),
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.prepared(request).is_none());
                assert!(scenes.diagnostics(request).unwrap().has_errors());
                assert!(scenes.acknowledge(request));
            });
            assert_last_good(world);
            assert_counts([1, 1, 0]);
            world.resource_mut::<Outcome>().acquisition_rejected = true;
            arm(DecodeAction::Hold);
            request_later(world, Stage::HeldWorker);
        }
        Stage::HeldWorker => {
            if !CODEC_PROBE.0.lock().unwrap().holding {
                yield_worker();
                return;
            }
            let request = pending(world);
            assert_counts([1, 2, 0]);
            assert_eq!(
                request_state(world, request),
                ProjectSceneRequestState::Preparing
            );
            ProjectScenes::scope(world, |scenes| {
                assert!(scenes.cancel(request));
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Cancelling),
                );
                assert!(!scenes.acknowledge(request));
                assert_eq!(
                    scenes.request(primary(LATER)),
                    Err(ProjectSceneRequestError::Busy),
                );
            });
            world.resource_mut::<Outcome>().stage = Stage::Cancelling;
        }
        Stage::Cancelling => {
            let request = pending(world);
            let state = request_state(world, request);
            if state == ProjectSceneRequestState::Cancelling {
                ProjectScenes::scope(world, |scenes| {
                    assert!(!scenes.acknowledge(request));
                    assert_eq!(
                        scenes.request(primary(LATER)),
                        Err(ProjectSceneRequestError::Busy),
                    );
                });
                // This frame's TaskUpdate has polled the cancelling request while decode is held.
                CODEC_PROBE.0.lock().unwrap().released = true;
                CODEC_PROBE.1.notify_all();
                yield_worker();
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Cancelled);
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert_last_good(world);
            // The original worker finished the closure before cancellation released its slot.
            assert_counts([1, 2, 1]);
            world.resource_mut::<Outcome>().cancellation_returned = true;
            request_later(world, Stage::RejectedCommit);
        }
        Stage::RejectedCommit => {
            let request = pending(world);
            if !ready(world, request) {
                return;
            }
            inspect_prepared(world, request);
            assert_counts([1, 3, 2]);
            let report = commit_prepared_project_scene(world, request, limits(), &[], |overlay| {
                overlay.replace_resource(CommitMarker::new(1));
                overlay.insert_component(entity("absent"), RuntimeTag);
            });
            assert!(report.instance.is_none());
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.product-overlay-target-missing"
            }));
            assert_eq!(
                request_state(world, request),
                ProjectSceneRequestState::Rejected(ProjectSceneRequestError::CommitRejected),
            );
            ProjectScenes::scope(world, |scenes| assert!(scenes.acknowledge(request)));
            assert_last_good(world);
            world.resource_mut::<Outcome>().commit_rejected = true;
            request_later(world, Stage::PrepareCommit);
        }
        Stage::PrepareCommit => {
            let request = pending(world);
            if !ready(world, request) {
                return;
            }
            let allocations = inspect_prepared(world, request);
            assert_counts([1, 4, 3]);
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.prepared_allocations = allocations;
            outcome.stage = Stage::HoldReady;
            // A complete ordinary frame ends with ready CPU data and the old live collections.
        }
        Stage::HoldReady => {
            let request = pending(world);
            let allocations = inspect_prepared(world, request);
            assert_eq!(
                allocations,
                world.resource::<Outcome>().prepared_allocations
            );
            assert_last_good(world);
            let report = commit_prepared_project_scene(world, request, limits(), &[], |overlay| {
                overlay.replace_resource(CommitMarker::new(1));
            });
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let receipt = report.instance.unwrap();
            let (current, published) = inspect_live(world, true);
            assert_eq!(current, receipt);
            assert_eq!(
                published, allocations,
                "the commit publishes the actual prepared acquisitions",
            );
            assert_ne!(
                Some(&receipt),
                world.resource::<Outcome>().startup_receipt.as_ref(),
            );
            assert_eq!(world.resource::<CommitMarker>().level, 1);
            assert_counts([1, 4, 3]);
            assert_source_free(world);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed),
                );
                assert!(scenes.acknowledge(request));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.committed_receipt = Some(receipt);
            outcome.request = None;
            outcome.stage = Stage::Done;
        }
        Stage::Done => unreachable!(),
    }
}

fn assert_last_good(world: &World) {
    let (receipt, allocations) = inspect_live(world, false);
    let outcome = world.resource::<Outcome>();
    assert_eq!(Some(&receipt), outcome.startup_receipt.as_ref());
    assert_eq!(allocations[0], outcome.startup_allocation);
    assert_eq!(world.resource::<CommitMarker>().level, 0);
}

fn assert_source_free(world: &World) {
    let probe = CODEC_PROBE.0.lock().unwrap();
    assert!(!probe.removed_root.as_ref().unwrap().exists());
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetDependencyGraph>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<AssetReloadRequests>());
    assert!(!world.contains_resource::<nara::image::ImageImporter>());
}

fn inspect_live(world: &World, later: bool) -> (SpawnedSceneInstance, [usize; 2]) {
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.source().roots, [entity("caption")]);
    let EntityLookup::Resolved(target) = current.receipt().resolve(world, &entity("caption"))
    else {
        panic!("the committed Caption must resolve through its actual Scene receipt")
    };
    let caption = world.get::<Caption>(target).unwrap();
    let source = if later { BUNDLE_B } else { BUNDLE_A };
    assert_eq!(caption.first.as_product(), Some(selected(source, FIRST)));
    assert_eq!(caption.second.as_product(), Some(selected(source, SECOND)));
    assert_eq!(
        caption.subtitle.as_product(),
        Some(selected(if later { BUNDLE_C } else { BUNDLE_A }, FIRST)),
    );
    (
        current.receipt().clone(),
        inspect_collections(world.resource::<ProjectDomainAssets>(), later),
    )
}

fn inspect_prepared(world: &mut World, request: ProjectSceneRequestId) -> [usize; 2] {
    let expected = world.resource::<Outcome>().startup_receipt.clone().unwrap();
    ProjectScenes::scope(world, |scenes| {
        let view: PreparedProjectSceneView<'_> = scenes.prepared(request).unwrap();
        assert_eq!(
            view.selected_scene(),
            &ProjectSceneSelection::Product(primary(LATER)),
        );
        assert_eq!(view.selected_scene_path(), None);
        assert_eq!(view.selected_scene_product(), Some(primary(LATER)));
        assert_eq!(view.expected_instance(), &expected);
        assert_eq!(view.scene().roots, [entity("caption")]);
        inspect_collections(view.domain_assets(), true)
    })
}

fn inspect_collections(assets: &ProjectDomainAssets, later: bool) -> [usize; 2] {
    let collection = assets.collection::<TextBundleDomain>().unwrap();
    assert_eq!(collection.source_records().len(), 0);
    assert_eq!(collection.iter().len(), if later { 2 } else { 1 });
    assert!(
        collection
            .get(&AssetRef::Path(
                AssetPath::new("authoring/a.bundle.json").unwrap()
            ))
            .is_none()
    );
    assert!(
        collection
            .get(&AssetRef::Product(selected(ABSENT, FIRST)))
            .is_none()
    );
    let first = inspect_bundle(
        collection,
        if later { BUNDLE_B } else { BUNDLE_A },
        if later { "B" } else { "A" },
    );
    assert!(
        collection
            .get(&AssetRef::Product(primary(if later {
                BUNDLE_A
            } else {
                BUNDLE_B
            })))
            .is_none()
    );
    let second = if later {
        inspect_bundle(collection, BUNDLE_C, "C")
    } else {
        0
    };
    if later {
        assert_ne!(
            first, second,
            "different format members remain distinct decoded values",
        );
    }
    [first, second]
}

fn inspect_bundle(
    collection: &ProjectAssetCollection<TextBundle>,
    source: &str,
    label: &str,
) -> usize {
    let first = selected(source, FIRST);
    let second = selected(source, SECOND);
    let bundle = collection.get(&AssetRef::Product(first)).unwrap();
    assert!(std::ptr::eq(
        bundle,
        collection.get(&AssetRef::Product(second)).unwrap(),
    ));
    assert!(std::ptr::eq(
        bundle,
        collection.get(&AssetRef::Product(primary(source))).unwrap(),
    ));
    assert_eq!(
        bundle.get(&first).unwrap().text,
        format!("{label} opening runtime"),
    );
    assert_eq!(
        bundle.get(&second).unwrap().text,
        format!("{label} ending runtime"),
    );
    bundle as *const TextBundle as usize
}

fn request_later(world: &mut World, stage: Stage) {
    let request = ProjectScenes::scope(world, |scenes| scenes.request(primary(LATER))).unwrap();
    let mut outcome = world.resource_mut::<Outcome>();
    outcome.request = Some(request);
    outcome.stage = stage;
}

fn pending(world: &World) -> ProjectSceneRequestId {
    world.resource::<Outcome>().request.unwrap()
}

fn request_state(world: &mut World, request: ProjectSceneRequestId) -> ProjectSceneRequestState {
    ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap()
}

fn ready(world: &mut World, request: ProjectSceneRequestId) -> bool {
    match request_state(world, request) {
        ProjectSceneRequestState::Preparing => {
            yield_worker();
            false
        }
        ProjectSceneRequestState::Ready => true,
        state => panic!("processed Scene did not prepare: {state:?}"),
    }
}

fn yield_worker() {
    std::thread::sleep(Duration::from_millis(1));
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::new(2).unwrap(), ItemLimit::ONE)
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum DecodeAction {
    #[default]
    Normal,
    Reject,
    Hold,
}

#[derive(Default)]
struct CodecProbe {
    counts: [usize; 3],
    next: DecodeAction,
    holding: bool,
    released: bool,
    removed_root: Option<PathBuf>,
}

static CODEC_PROBE: (Mutex<CodecProbe>, Condvar) = (
    Mutex::new(CodecProbe {
        counts: [0; 3],
        next: DecodeAction::Normal,
        holding: false,
        released: false,
        removed_root: None,
    }),
    Condvar::new(),
);

fn arm(action: DecodeAction) {
    let mut probe = CODEC_PROBE.0.lock().unwrap();
    assert!(probe.next == DecodeAction::Normal && !probe.holding);
    probe.next = action;
    probe.released = false;
}

fn assert_counts(expected: [usize; 3]) {
    assert_eq!(CODEC_PROBE.0.lock().unwrap().counts, expected);
}

/// Both formats delegate actual domain decoding, cost and Product membership to the library.
/// The alternate format changes only the envelope kind, exercising one family's format routing.
struct ObservedCodec<const ALTERNATE: bool>;

impl<const ALTERNATE: bool> RuntimeAssetCodec<TextBundle> for ObservedCodec<ALTERNATE> {
    const RUNTIME_KIND: &'static str = if ALTERNATE {
        "consumer_text_bundle_alternate"
    } else {
        TextBundleDomain::RUNTIME_KIND
    };
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        let original = TextBundleDomain::runtime_limits();
        // The alternate adapter owns a bounded JSON value and translated serializer buffer too.
        AssetRuntimeLimits::new(
            original.encoded_bytes(),
            ByteLimit::new(16 * 1024 * 1024).unwrap(),
            original.retained_bytes(),
        )
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        let encoded = TextBundleDomain::encode_runtime(product)?;
        if ALTERNATE {
            relabel(&encoded, TextBundleDomain::RUNTIME_KIND, Self::RUNTIME_KIND)
                .map(Vec::into_boxed_slice)
        } else {
            Ok(encoded)
        }
    }

    fn decode_runtime(encoded: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        let product = if ALTERNATE {
            TextBundleDomain::decode_runtime(&relabel(
                encoded,
                Self::RUNTIME_KIND,
                TextBundleDomain::RUNTIME_KIND,
            )?)?
        } else {
            TextBundleDomain::decode_runtime(encoded)?
        };
        let mut probe = CODEC_PROBE.0.lock().unwrap();
        let index = if product.source() == stable(BUNDLE_A) {
            0
        } else if product.source() == stable(BUNDLE_B) {
            1
        } else if product.source() == stable(BUNDLE_C) {
            2
        } else {
            return Err(codec_error());
        };
        probe.counts[index] += 1;
        if index == 1 {
            match std::mem::take(&mut probe.next) {
                DecodeAction::Normal => {}
                DecodeAction::Reject => return Err(codec_error()),
                DecodeAction::Hold => {
                    probe.holding = true;
                    let (mut probe, timeout) = CODEC_PROBE
                        .1
                        .wait_timeout_while(probe, Duration::from_secs(5), |probe| !probe.released)
                        .unwrap();
                    probe.holding = false;
                    if timeout.timed_out() && !probe.released {
                        return Err(codec_error());
                    }
                }
            }
        }
        Ok(product)
    }

    fn runtime_retained_bytes(product: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(product)
    }

    fn visit_required_runtime_products<E>(
        product: &TextBundle,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        TextBundleDomain::visit_required_runtime_products(product, visitor)
    }

    fn contains_runtime_product(product: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(product, reference)
    }
}

fn relabel(
    encoded: &[u8],
    expected: &str,
    replacement: &str,
) -> Result<Vec<u8>, AssetRuntimeError> {
    if encoded.len() > TextBundleDomain::runtime_limits().encoded_bytes().get() {
        return Err(codec_error());
    }
    let mut document: serde_json::Value =
        serde_json::from_slice(encoded).map_err(|_| codec_error())?;
    if document.get("kind").and_then(serde_json::Value::as_str) != Some(expected) {
        return Err(codec_error());
    }
    document["kind"] = serde_json::Value::String(replacement.into());
    let result = serde_json::to_vec(&document).map_err(|_| codec_error())?;
    if result.len() > TextBundleDomain::runtime_limits().encoded_bytes().get() {
        return Err(codec_error());
    }
    Ok(result)
}

fn codec_error() -> AssetRuntimeError {
    AssetRuntimeError::new(Diagnostic::error(
        DiagnosticCode::new("consumer.processed-host.decode-refused").unwrap(),
        SafeSummary::new("The independent Runtime test codec refused this candidate").unwrap(),
    ))
}

#[test]
fn source_free_headless_host_keeps_last_good_and_commits_compound_collections() {
    let base = Fixture::new().capture_and_remove();
    assert_counts([0, 0, 0]);
    let options = HeadlessRunOptions::frames(NonZeroU32::new(20_000).unwrap(), Duration::ZERO)
        .stop_when(|outcome: &Outcome| outcome.stage == Stage::Done);
    let mut run = HeadlessRun::<Outcome>::from_runtime_content(base, options, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                assert!(
                    !report.diagnostics().has_errors(),
                    "{:?}",
                    report.diagnostics()
                );
                assert!(outcome.acquisition_rejected);
                assert!(outcome.cancellation_returned);
                assert!(outcome.commit_rejected);
                assert!(outcome.source_free_frames >= 7);
                assert!(outcome.committed_receipt.is_some());
                assert_counts([1, 4, 3]);
                break;
            }
            HeadlessRunOutcome::Failed => {
                panic!("processed Host failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "processed Host cleanup did not return",
                );
                yield_worker();
            }
        }
    }
}

struct Fixture {
    root: PathBuf,
    expected: RuntimeContentExpectation,
    recipe: ProductRecipe,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara-processed-host-consumer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("authoring")).unwrap();
        fs::create_dir(root.join("runtime")).unwrap();
        fs::write(root.join("authoring/suffix.txt"), b" runtime").unwrap();
        let recipe = recipe();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let mut members = Vec::new();
        let mut database = ProjectAssetDatabase::default();
        for (source, label, path, alternate) in [
            (BUNDLE_A, "A", "runtime/10-a.bundle.json", false),
            (BUNDLE_B, "B", "runtime/20-b.bundle.json", false),
            (BUNDLE_C, "C", "runtime/30-c.bundle.json", true),
        ] {
            let bundle = import_bundle(&root, source, label);
            let encoded = if alternate {
                ObservedCodec::<true>::encode_runtime(&bundle)
            } else {
                ObservedCodec::<false>::encode_runtime(&bundle)
            }
            .unwrap();
            fs::write(root.join(path), &encoded).unwrap();
            let format = if alternate {
                ObservedCodec::<true>::RUNTIME_KIND
            } else {
                ObservedCodec::<false>::RUNTIME_KIND
            };
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(format).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![
                        primary(source),
                        selected(source, FIRST),
                        selected(source, SECOND),
                    ],
                    vec![],
                )
                .unwrap(),
            );
            database
                .insert(AssetRecord::new(
                    stable(source),
                    nara::asset::AssetSourcePath::new(format!(
                        "authoring/{}.bundle.json",
                        label.to_lowercase()
                    ))
                    .unwrap(),
                    AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
                ))
                .unwrap();
        }
        for (reference, later, path) in [
            (primary(STARTUP), false, "runtime/startup.scene.json"),
            (primary(LATER), true, "runtime/later.scene.json"),
        ] {
            let encoded = scene_bytes(&root, &plan, &database, reference, later);
            fs::write(root.join(path), &encoded).unwrap();
            let sources: &[&str] = if later {
                &[BUNDLE_B, BUNDLE_C]
            } else {
                &[BUNDLE_A]
            };
            let dependencies = sources
                .iter()
                .flat_map(|source| {
                    [FIRST, SECOND].map(|id| {
                        RuntimeContentDependency::new(
                            selected(source, id),
                            RuntimeContentDependencyRole::Required,
                        )
                    })
                })
                .collect();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: SourceDocumentKey::stable_asset(
                            reference.source(),
                            SourceDocumentKind::Scene,
                        ),
                    },
                    vec![reference],
                    dependencies,
                )
                .unwrap(),
            );
        }
        let target = RuntimeContentTarget::new("test-host", "processed-consumer").unwrap();
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
                startup_scene: primary(STARTUP),
                scenes: vec![primary(STARTUP), primary(LATER)],
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
        let root = checked_fixture_root(&self.root);
        fs::remove_dir_all(&root).unwrap();
        assert!(!root.exists());
        CODEC_PROBE.0.lock().unwrap().removed_root = Some(root);
        base
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.root.exists() {
            fs::remove_dir_all(checked_fixture_root(&self.root)).unwrap();
        }
    }
}

fn checked_fixture_root(root: &Path) -> PathBuf {
    let root = root.canonicalize().unwrap();
    assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    assert!(
        root.file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("nara-processed-host-consumer-")
    );
    root
}

fn import_bundle(root: &Path, source: &str, label: &str) -> TextBundle {
    let raw = serde_json::to_vec(&serde_json::json!({
        "suffix": "suffix.txt",
        "entries": [
            {"name": "Opening", "text": format!("{label} opening")},
            {"name": "Ending", "text": format!("{label} ending")},
        ],
    }))
    .unwrap();
    let path = format!("authoring/{}.bundle.json", label.to_lowercase());
    fs::write(root.join(&path), &raw).unwrap();
    let settings =
        TextBundleSettings::initial(&raw, 64, [auxiliary(FIRST), auxiliary(SECOND)]).unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            stable(source),
            AssetPath::new(path.clone()).unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        settings,
    )
    .to_json_string()
    .unwrap();
    fs::write(root.join(format!("{path}.meta")), &metadata).unwrap();
    let candidate = AssetMetaIdentityCandidate::decode_json_bytes(metadata.as_bytes()).unwrap();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        let bytes = fs::read(root.join("authoring").join(path.as_str()))?;
        assert!(bytes.len() <= limit.get());
        Ok::<Arc<[u8]>, std::io::Error>(Arc::from(bytes))
    };
    TextBundleDomain::prepare(&mut AssetImportSource::new(
        &candidate,
        Arc::from(raw),
        &mut reader,
    ))
    .unwrap()
}

fn scene_bytes(
    root: &Path,
    plan: &RuntimeConfigurationPlan,
    database: &ProjectAssetDatabase,
    reference: AssetProductRef,
    later: bool,
) -> Vec<u8> {
    let source = if later { BUNDLE_B } else { BUNDLE_A };
    let caption = Caption {
        first: AssetRef::Product(selected(source, FIRST)),
        second: AssetRef::Product(selected(source, SECOND)),
        subtitle: AssetRef::Product(selected(if later { BUNDLE_C } else { BUNDLE_A }, FIRST)),
    };
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(entity("caption"))
        .with_component(
            ComponentTypeId::new(CAPTION),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                caption.__encode_persistent_component().unwrap(),
            ),
        )]);
    let path = if later {
        "authoring/later.scene.json"
    } else {
        "authoring/startup.scene.json"
    };
    fs::write(root.join(path), document.to_json_string().unwrap()).unwrap();
    let expanded = ExpandedSceneCandidate::plain(
        SourceDocumentKey::stable_asset(reference.source(), SourceDocumentKind::Scene),
        Arc::new(document),
    );
    RuntimeSceneCandidate::cook(
        &expanded,
        plan.schema_validation().bindings(),
        database.resolver(&nara::asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

fn entity(value: &str) -> SceneEntityId {
    SceneEntityId::new(value).unwrap()
}

fn stable(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn auxiliary(value: &str) -> ImportedProductId {
    ImportedProductId::parse_subproduct(value).unwrap()
}

fn primary(source: &str) -> AssetProductRef {
    AssetProductRef::new(stable(source), ImportedProductId::PRIMARY).unwrap()
}

fn selected(source: &str, id: &str) -> AssetProductRef {
    AssetProductRef::new(stable(source), auxiliary(id)).unwrap()
}
