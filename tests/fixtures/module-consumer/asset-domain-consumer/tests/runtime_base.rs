//! Independent processed-content consumption, without a source project or import-cache fallback.
//!
//! CPU bundle admission and managed Scene publication are separate proofs. This fixture does not
//! install a generic Project Host or claim automatic native resource residency for every family.

use std::{
    fs,
    mem::size_of,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};

use nara::{
    ProductRecipe, SchemaContribution,
    advanced_prelude::{
        CurrentScene, SceneSource, materialize_startup_scene, replace_current_scene_with_product,
    },
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId, RuntimeAdmissionReservation, RuntimeClosePolicy,
        RuntimeObligationLedger,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetRecord, AssetRef,
        AssetSourceKind, ProjectAssetDatabase,
    },
    core::{ByteLimit, FormatKind, FormatVersion, ItemLimit},
    ecs::{Component, Entity, Resource, World},
    fs::ContentDigest,
    identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentAsset, RuntimeContentBase,
        RuntimeContentBudgetKind, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentScene, RuntimeContentTarget,
    },
    scene::{
        ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentBindings,
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, SourceDocumentKey,
        SourceDocumentKind, advanced::SceneProductTransactionLimits, scene_component_bindings,
    },
};
use nara_asset::{
    AssetDomain, AssetImportSource, AssetProductRef, AssetRuntimeError, AssetRuntimeLimits,
    ImportedProductId, RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
};
use nara_asset_domain_consumer::{TextBundle, TextBundleDomain, TextBundleSettings};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};

const BUNDLE_SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const OTHER_SOURCE: &str = "af3176df-4556-45a5-9b2f-334ce47a5fbf";
const STARTUP_SOURCE: &str = "b9cf8d3e-8776-4f29-a67d-fd902e7398b9";
const LATER_SOURCE: &str = "b0e10861-31ef-4694-9d82-9a5131d6dc46";
const FIRST: &str = "10a5c635-e71b-478d-86c4-ef933956a425";
const SECOND: &str = "f319f4c7-d809-40d9-8c97-4abb6c68b6da";
const ABSENT: &str = "4d42363d-ea9f-4920-b3c7-b141a7b15d98";
const STARTUP_FILE: &str = "startup.runtime.json";
const LATER_FILE: &str = "later.runtime.json";
const BUNDLE_FILE: &str = "bundle.runtime.json";
const CAPTION: &str = "consumer.runtime-base.Caption";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;
const SINGLE_RAW: &[u8] =
    br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"}]}"#;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.runtime-base.Caption",
    version = 1,
    alias = "Runtime Caption",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
struct Caption {
    #[nara(
        id = "product",
        alias = "Product",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    product: AssetRef,
    #[nara(id = "samples", alias = "Samples", capabilities(scene, inspect, edit))]
    samples: Vec<i64>,
}

nara::persistent_components! { mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.runtime-base.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.runtime-base"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.runtime-base.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct ConsumerPlugin;

impl Plugin for ConsumerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.runtime-base"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    // Runtime codec selection below does not select the source importer in the executable recipe.
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<ConsumerPlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
}

fn target() -> RuntimeContentTarget {
    RuntimeContentTarget::new("x86_64-pc-windows-msvc", "consumer-test").unwrap()
}

fn stable(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn auxiliary(value: &str) -> ImportedProductId {
    ImportedProductId::parse_subproduct(value).unwrap()
}

fn product(source: &str, id: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(stable(source), id).unwrap()
}

fn selected(value: &str) -> AssetProductRef {
    product(BUNDLE_SOURCE, auxiliary(value))
}

fn startup_product() -> AssetProductRef {
    product(STARTUP_SOURCE, ImportedProductId::PRIMARY)
}

fn later_product() -> AssetProductRef {
    product(LATER_SOURCE, ImportedProductId::PRIMARY)
}

fn caption_id() -> SceneEntityId {
    SceneEntityId::new("caption").unwrap()
}

fn bytes(value: usize) -> ByteLimit {
    ByteLimit::new(value).unwrap()
}

fn bundle_bytes(source: &str, raw: &[u8], products: &[ImportedProductId]) -> Vec<u8> {
    let settings = TextBundleSettings::initial(raw, 64, products.iter().copied()).unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            stable(source),
            AssetPath::new("authoring/bundle.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        settings,
    );
    let encoded = metadata.to_json_string().unwrap();
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    let suffix: Arc<[u8]> = Arc::from(b" world".as_slice());
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        assert_eq!(path.as_str(), "suffix.txt");
        assert!(suffix.len() <= limit.get());
        Ok::<_, std::convert::Infallible>(Arc::clone(&suffix))
    };
    let mut source = AssetImportSource::new(&metadata, Arc::from(raw), &mut reader);
    let bundle = TextBundleDomain::prepare(&mut source).unwrap();
    TextBundleDomain::encode_runtime(&bundle)
        .unwrap()
        .into_vec()
}

fn scene_bytes(
    bindings: &SceneComponentBindings,
    source: &str,
    selection: AssetProductRef,
) -> Vec<u8> {
    scene_bytes_with_samples(bindings, source, selection, vec![])
}

fn scene_bytes_with_samples(
    bindings: &SceneComponentBindings,
    source: &str,
    selection: AssetProductRef,
    samples: Vec<i64>,
) -> Vec<u8> {
    let caption = Caption {
        product: AssetRef::Product(selection),
        samples,
    };
    let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(caption_id())
        .with_component(
            ComponentTypeId::new(CAPTION),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                caption.__encode_persistent_component().unwrap(),
            ),
        )]);
    let expanded = ExpandedSceneCandidate::plain(
        SourceDocumentKey::stable_asset(stable(source), SourceDocumentKind::Scene),
        Arc::new(document),
    );
    let mut database = ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            stable(BUNDLE_SOURCE),
            AssetPath::new("authoring/bundle.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ))
        .unwrap();
    RuntimeSceneCandidate::cook(
        &expanded,
        bindings,
        &database,
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

struct Fixture {
    root: PathBuf,
    catalog: RuntimeContentCatalog,
    expected: RuntimeContentExpectation,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-runtime-base-consumer-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe()).unwrap();
        let startup = scene_bytes(
            plan.schema_validation().bindings(),
            STARTUP_SOURCE,
            selected(FIRST),
        );
        let later = scene_bytes(
            plan.schema_validation().bindings(),
            LATER_SOURCE,
            selected(SECOND),
        );
        let bundle = bundle_bytes(BUNDLE_SOURCE, RAW, &[auxiliary(FIRST), auxiliary(SECOND)]);
        let mut members = Vec::new();
        for (path, encoded, source, reference, dependency) in [
            (
                STARTUP_FILE,
                startup,
                STARTUP_SOURCE,
                startup_product(),
                selected(FIRST),
            ),
            (
                LATER_FILE,
                later,
                LATER_SOURCE,
                later_product(),
                selected(SECOND),
            ),
        ] {
            fs::write(root.join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: SourceDocumentKey::stable_asset(
                            stable(source),
                            SourceDocumentKind::Scene,
                        ),
                    },
                    vec![reference],
                    vec![RuntimeContentDependency::new(
                        dependency,
                        RuntimeContentDependencyRole::Required,
                    )],
                )
                .unwrap(),
            );
        }
        fs::write(root.join(BUNDLE_FILE), &bundle).unwrap();
        members.push(
            RuntimeContentMember::new(
                BUNDLE_FILE,
                ContentDigest::of_bytes(&bundle),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(TextBundleDomain::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::ONE,
                },
                vec![
                    product(BUNDLE_SOURCE, ImportedProductId::PRIMARY),
                    selected(FIRST),
                    selected(SECOND),
                ],
                vec![],
            )
            .unwrap(),
        );
        let definition = RuntimeContentCatalogDefinition {
            target: target(),
            configuration,
            plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
            schema_fingerprint: plan
                .schema_validation()
                .composition_fingerprint()
                .as_bytes(),
            startup_scene: startup_product(),
            scenes: vec![startup_product(), later_product()],
            addressable_roots: vec![selected(FIRST), selected(SECOND)],
            members,
        };
        let catalog =
            RuntimeContentCatalog::new(definition, RuntimeContentCatalogLimits::default()).unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            catalog,
            // These expectations are producer outputs supplied independently to the reader.
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target()),
        }
    }

    fn open(
        &self,
        limits: RuntimeContentLimits,
    ) -> Result<RuntimeContentBase, RuntimeContentError> {
        let directory = open_explicit_product_project_root(&self.root).unwrap();
        RuntimeContentBase::open(&directory, &self.expected, recipe(), limits)
    }

    fn rewrite_catalog(&mut self, edit: impl FnOnce(&mut RuntimeContentCatalogDefinition)) {
        let mut definition = RuntimeContentCatalogDefinition {
            target: self.catalog.target().clone(),
            configuration: self.catalog.configuration().clone(),
            plugin_plan_fingerprint: self.catalog.plugin_plan_fingerprint(),
            schema_fingerprint: self.catalog.schema_fingerprint(),
            startup_scene: self.catalog.startup_scene(),
            scenes: self.catalog.scenes().to_vec(),
            addressable_roots: self.catalog.addressable_roots().to_vec(),
            members: self.catalog.members().to_vec(),
        };
        edit(&mut definition);
        self.catalog =
            RuntimeContentCatalog::new(definition, RuntimeContentCatalogLimits::default()).unwrap();
        let encoded = self.catalog.to_json_bytes().unwrap();
        fs::write(self.root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        self.expected = RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target());
    }

    fn replace_bundle(&mut self, encoded: &[u8]) {
        fs::write(self.root.join(BUNDLE_FILE), encoded).unwrap();
        self.rewrite_catalog(|definition| {
            let member = definition
                .members
                .iter_mut()
                .find(|member| member.path() == BUNDLE_FILE)
                .unwrap();
            *member = RuntimeContentMember::new(
                BUNDLE_FILE,
                ContentDigest::of_bytes(encoded),
                member.kind().clone(),
                member.products().to_vec(),
                member.dependencies().to_vec(),
            )
            .unwrap();
        });
    }

    fn open_for_one_asset(&self) -> RuntimeContentBase {
        let limits = RuntimeContentLimits::default()
            .with_retained_bytes(TextBundleDomain::runtime_limits().retained_bytes());
        let probe = self.open(limits).unwrap();
        let required =
            match probe.acquire_asset::<TextBundleDomain, PanickingDecode>(selected(FIRST)) {
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                    required,
                    ..
                }) => required,
                result => {
                    panic!("expected a predecode retained-output reservation refusal: {result:?}")
                }
            };
        self.open(limits.with_retained_bytes(bytes(required)))
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
                .starts_with("nara-runtime-base-consumer-"),
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[derive(Resource)]
struct Travel {
    next: Option<RuntimeContentScene>,
    foreign: Option<SceneSource>,
    bundle: RuntimeContentAsset<TextBundle>,
    completed: bool,
}

fn resolve(world: &World, receipt: &SpawnedSceneInstance) -> Entity {
    match receipt.resolve(world, &caption_id()) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live Caption: {other:?}"),
    }
}

fn caption_text(world: &World, receipt: &SpawnedSceneInstance) -> String {
    let entity = resolve(world, receipt);
    let reference = world
        .get::<Caption>(entity)
        .unwrap()
        .product
        .as_product()
        .unwrap();
    world
        .resource::<Travel>()
        .bundle
        .product()
        .get(&reference)
        .unwrap()
        .text
        .clone()
}

fn travel(world: &mut World) {
    let (next, foreign) = {
        let Some(mut state) = world.get_resource_mut::<Travel>() else {
            return;
        };
        if state.completed {
            return;
        }
        (state.next.take().unwrap(), state.foreign.take().unwrap())
    };
    let current = CurrentScene::get(world).unwrap();
    let initial = current.receipt().clone();
    let initial_view = current.source_view();
    let old_entity = resolve(world, &initial);
    assert_eq!(caption_text(world, &initial), "Hello world");
    assert!(initial_view.with_expanded_scene(|_, _| ()).is_none());
    assert!(initial_view.with_runtime_scene(|_| ()).is_some());
    let identity = world.resource::<WorldIdentityDomain>().stats();
    let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE);
    let rejected =
        replace_current_scene_with_product(world, &initial, foreign, limits, &[], |_| {
            panic!("foreign definitions must refuse before product configuration")
        });
    assert!(rejected.instance.is_none());
    assert!(rejected.diagnostics.has_errors());
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &initial);
    assert_eq!(resolve(world, &initial), old_entity);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(caption_text(world, &initial), "Hello world");
    assert!(initial_view.with_runtime_scene(|_| ()).is_some());
    let committed = replace_current_scene_with_product(
        world,
        &initial,
        next.scene_source(),
        limits,
        &[],
        |_| {},
    );
    assert!(!committed.diagnostics.has_errors(), "{committed:?}");
    let replacement = committed.instance.unwrap();
    assert_eq!(CurrentScene::get(world).unwrap().receipt(), &replacement);
    assert_eq!(caption_text(world, &replacement), "Goodbye world");
    assert!(world.get_entity(old_entity).is_err());
    assert!(initial_view.with_document(|_| ()).is_none());
    let current = CurrentScene::get(world).unwrap();
    assert!(
        current
            .source_view()
            .with_runtime_scene(|scene| {
                scene
                    .definitions()
                    .matches(scene_component_bindings(world).unwrap())
            })
            .unwrap(),
    );
    // The default AssetPlugin installs an empty table for its source-change systems. This
    // consumer never supplies source metadata or uses that table to resolve Runtime products.
    assert!(world.resource::<ProjectAssetDatabase>().is_empty());
    world.resource_mut::<Travel>().completed = true;
}

#[test]
fn captured_base_runs_two_managed_scenes_and_compound_products_after_all_files_change() {
    let fixture = Fixture::new();
    let base = fixture.open(RuntimeContentLimits::default()).unwrap();
    let independently_opened = fixture.open(RuntimeContentLimits::default()).unwrap();
    let plan = base.plan().clone();
    assert_eq!(
        plan.plugin_plan().fingerprint(),
        independently_opened.plan().plugin_plan().fingerprint(),
    );
    assert!(
        !plan
            .schema_validation()
            .bindings()
            .same_definitions(independently_opened.plan().schema_validation().bindings(),),
    );
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), 4);
    for path in [RUNTIME_CONTENT_CATALOG_FILE, STARTUP_FILE, BUNDLE_FILE] {
        fs::remove_file(fixture.root.join(path)).unwrap();
    }
    fs::write(fixture.root.join(LATER_FILE), b"not a Scene or a catalog").unwrap();

    let startup = base.acquire_startup_scene().unwrap();
    let later = base.acquire_scene(later_product()).unwrap();
    let bundle = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST))
        .unwrap();
    let second = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND))
        .unwrap();
    assert_eq!(startup.product(), startup_product());
    assert_eq!(later.product(), later_product());
    assert_eq!(bundle.reference(), selected(FIRST));
    assert_eq!(
        bundle.product().get(&selected(FIRST)).unwrap().text,
        "Hello world"
    );
    assert_eq!(
        second.product().get(&selected(SECOND)).unwrap().text,
        "Goodbye world"
    );
    assert_eq!(bundle.base().catalog_digest(), base.catalog_digest());
    assert!(
        startup
            .candidate()
            .definitions()
            .matches(plan.schema_validation().bindings())
    );
    assert!(
        later
            .candidate()
            .definitions()
            .matches(plan.schema_validation().bindings())
    );
    assert!(
        later
            .base()
            .plan()
            .schema_validation()
            .bindings()
            .same_definitions(plan.schema_validation().bindings(),)
    );
    let foreign = independently_opened.acquire_scene(later_product()).unwrap();
    drop(second);
    drop(independently_opened);
    drop(base);

    let mut candidate = plan
        .plugin_plan()
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let expected_bindings = plan.schema_validation().bindings().clone();
    let admission = candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                assert!(
                    expected_bindings.same_definitions(scene_component_bindings(world).unwrap()),
                );
                world.insert_resource(Travel {
                    next: Some(later),
                    foreign: Some(foreign.scene_source()),
                    bundle,
                    completed: false,
                });
                let diagnostics = materialize_startup_scene(world, startup.scene_source()).unwrap();
                assert!(!diagnostics.has_errors());
                assert!(CurrentScene::get(world).is_none());
                assert!(world.resource::<ProjectAssetDatabase>().is_empty());
            });
        })
        .unwrap();
    assert!(admission.post_operation_fault.is_none());
    let mut runtime = candidate.complete_startup().unwrap().promote();
    assert_eq!(
        caption_text(
            runtime.world(),
            CurrentScene::get(runtime.world()).unwrap().receipt()
        ),
        "Hello world",
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<Travel>().completed);
    assert_eq!(
        caption_text(
            runtime.world(),
            CurrentScene::get(runtime.world()).unwrap().receipt()
        ),
        "Goodbye world",
    );
    runtime.finish().unwrap();
}

#[test]
fn host_digest_target_recipe_and_schema_are_independent_admission_requirements() {
    let mut fixture = Fixture::new();
    let directory = open_explicit_product_project_root(&fixture.root).unwrap();
    let original = fixture.expected.catalog_digest();
    let mut changed_hash = *original.as_bytes();
    changed_hash[0] ^= 1;
    let wrong_digest = RuntimeContentExpectation::new(
        ContentDigest::from_parts(original.length(), changed_hash),
        target(),
    );
    assert!(matches!(
        RuntimeContentBase::open(
            &directory,
            &wrong_digest,
            recipe(),
            RuntimeContentLimits::default()
        ),
        Err(RuntimeContentError::CatalogDigestMismatch),
    ));
    let wrong_target = RuntimeContentExpectation::new(
        original,
        RuntimeContentTarget::new("aarch64-unknown-linux-gnu", "consumer-test").unwrap(),
    );
    assert!(matches!(
        RuntimeContentBase::open(
            &directory,
            &wrong_target,
            recipe(),
            RuntimeContentLimits::default()
        ),
        Err(RuntimeContentError::TargetMismatch),
    ));
    assert!(matches!(
        RuntimeContentBase::open(
            &directory,
            &fixture.expected,
            ProductRecipe::new(),
            RuntimeContentLimits::default(),
        ),
        Err(RuntimeContentError::RecipeMismatch),
    ));
    let original_schema = fixture.catalog.schema_fingerprint();
    fixture.rewrite_catalog(|definition| definition.schema_fingerprint[0] ^= 1);
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default()),
        Err(RuntimeContentError::SchemaMismatch),
    ));
    fixture.rewrite_catalog(|definition| definition.schema_fingerprint = original_schema);
    assert!(fixture.open(RuntimeContentLimits::default()).is_ok());
}

#[test]
fn every_member_is_present_and_digest_verified_before_a_base_can_publish() {
    let fixture = Fixture::new();
    let later = fs::read(fixture.root.join(LATER_FILE)).unwrap();
    fs::remove_file(fixture.root.join(LATER_FILE)).unwrap();
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default()),
        Err(RuntimeContentError::Host(_)),
    ));
    fs::write(fixture.root.join(LATER_FILE), later).unwrap();
    let original = fs::read(fixture.root.join(BUNDLE_FILE)).unwrap();
    let mut damaged = original.clone();
    damaged[0] ^= 1;
    fs::write(fixture.root.join(BUNDLE_FILE), damaged).unwrap();
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default()),
        Err(RuntimeContentError::MemberDigestMismatch),
    ));
    fs::write(fixture.root.join(BUNDLE_FILE), original).unwrap();
    let recovered = fixture.open(RuntimeContentLimits::default()).unwrap();
    assert!(recovered.acquire_startup_scene().is_ok());
    assert!(recovered.acquire_scene(later_product()).is_ok());
}

#[test]
fn a_valid_codec_payload_cannot_borrow_another_source_or_omit_a_declared_auxiliary() {
    for encoded in [
        bundle_bytes(OTHER_SOURCE, RAW, &[auxiliary(FIRST), auxiliary(SECOND)]),
        bundle_bytes(BUNDLE_SOURCE, SINGLE_RAW, &[auxiliary(FIRST)]),
    ] {
        let mut fixture = Fixture::new();
        let last_good = fixture.open(RuntimeContentLimits::default()).unwrap();
        fixture.replace_bundle(&encoded);
        let rejected = fixture.open(RuntimeContentLimits::default()).unwrap();
        // FIRST exists in both payloads. Admission checks all mapped identities, including SECOND
        // and the enclosing source, rather than blessing only the requested auxiliary.
        assert!(matches!(
            rejected.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST)),
            Err(RuntimeContentError::ProductIdentityMismatch),
        ));
        let original = last_good
            .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND))
            .unwrap();
        assert_eq!(original.product().source(), stable(BUNDLE_SOURCE));
        assert_eq!(
            original.product().get(&selected(SECOND)).unwrap().text,
            "Goodbye world"
        );
        assert!(matches!(
            last_good.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(ABSENT)),
            Err(RuntimeContentError::UndeclaredProduct),
        ));
    }
}

#[test]
fn actual_scene_asset_leaves_require_membership_and_an_explicit_required_edge() {
    for (absent_product, role) in [
        (false, None),
        (false, Some(RuntimeContentDependencyRole::Soft)),
        (true, Some(RuntimeContentDependencyRole::Soft)),
    ] {
        let mut fixture = Fixture::new();
        fixture.rewrite_catalog(|definition| {
            let later = definition
                .members
                .iter_mut()
                .find(|member| member.path() == LATER_FILE)
                .unwrap();
            *later = RuntimeContentMember::new(
                later.path(),
                later.digest(),
                later.kind().clone(),
                later.products().to_vec(),
                role.into_iter()
                    .map(|role| RuntimeContentDependency::new(selected(SECOND), role))
                    .collect(),
            )
            .unwrap();
            if absent_product {
                let bundle = definition
                    .members
                    .iter_mut()
                    .find(|member| member.path() == BUNDLE_FILE)
                    .unwrap();
                *bundle = RuntimeContentMember::new(
                    bundle.path(),
                    bundle.digest(),
                    bundle.kind().clone(),
                    bundle
                        .products()
                        .iter()
                        .copied()
                        .filter(|product| *product != selected(SECOND))
                        .collect(),
                    bundle.dependencies().to_vec(),
                )
                .unwrap();
                definition
                    .addressable_roots
                    .retain(|product| *product != selected(SECOND));
            }
        });
        // The alleged graph is valid: soft declarations may be unresolved. The admitted Scene
        // still contains the real derived Caption AssetRef leaf and must establish its own demand.
        let base = fixture.open(RuntimeContentLimits::default()).unwrap();
        let initial = base.acquire_startup_scene().unwrap();
        let result = base.acquire_scene(later_product());
        if absent_product {
            assert!(matches!(
                result,
                Err(RuntimeContentError::UndeclaredProduct)
            ));
        } else {
            assert!(matches!(
                result,
                Err(RuntimeContentError::UndeclaredDependency)
            ));
        }
        assert!(
            initial
                .candidate()
                .definitions()
                .matches(base.plan().schema_validation().bindings())
        );
        assert!(base.acquire_startup_scene().is_ok());
        assert_eq!(
            base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST))
                .unwrap()
                .product()
                .get(&selected(FIRST))
                .unwrap()
                .text,
            "Hello world",
        );
    }
}

struct PanickingMembership;

impl AssetDomain for PanickingMembership {
    type Product = TextBundle;

    fn contains_product(_: &TextBundle, _: ImportedProductId) -> bool {
        panic!("consumer membership query failed");
    }
}

struct PanickingDecode;

impl RuntimeAssetCodec<TextBundle> for PanickingDecode {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TextBundleDomain::runtime_limits()
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(product)
    }

    fn decode_runtime(_: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        panic!("consumer Runtime decoder failed");
    }

    fn runtime_retained_bytes(product: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(product)
    }

    fn contains_runtime_product(product: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(product, reference)
    }
}

struct PanickingIdentity;

impl RuntimeAssetCodec<TextBundle> for PanickingIdentity {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TextBundleDomain::runtime_limits()
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(product)
    }

    fn decode_runtime(encoded: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        TextBundleDomain::decode_runtime(encoded)
    }

    fn runtime_retained_bytes(product: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(product)
    }

    fn contains_runtime_product(_: &TextBundle, _: AssetProductRef) -> bool {
        panic!("consumer complete Runtime identity query failed");
    }
}

struct WrongCodec;

impl RuntimeAssetCodec<TextBundle> for WrongCodec {
    const RUNTIME_KIND: &'static str = "consumer_another_bundle_format";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        panic!("codec identity must refuse before requesting its promises");
    }

    fn encode_runtime(_: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        panic!("the wrong codec must not encode");
    }

    fn decode_runtime(_: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        panic!("the wrong codec must not decode");
    }

    fn runtime_retained_bytes(_: &TextBundle) -> Result<usize, AssetRuntimeError> {
        panic!("the wrong codec must not query retained output");
    }

    fn contains_runtime_product(_: &TextBundle, _: AssetProductRef) -> bool {
        panic!("the wrong codec must not inspect a product");
    }
}

struct InvalidRetainedCost<const MODE: u8>;

impl<const MODE: u8> RuntimeAssetCodec<TextBundle> for InvalidRetainedCost<MODE> {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        let limits = TextBundleDomain::runtime_limits();
        if MODE == 4 {
            AssetRuntimeLimits::new(
                limits.encoded_bytes(),
                limits.work_bytes(),
                bytes(size_of::<TextBundle>() - 1),
            )
        } else {
            limits
        }
    }

    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(product)
    }

    fn decode_runtime(encoded: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        assert_ne!(MODE, 4, "an under-inline promise must refuse before decode");
        TextBundleDomain::decode_runtime(encoded)
    }

    fn runtime_retained_bytes(_: &TextBundle) -> Result<usize, AssetRuntimeError> {
        match MODE {
            0 => panic!("consumer Runtime retained-cost query failed"),
            1 => Err(AssetRuntimeError::new(Diagnostic::error(
                DiagnosticCode::new("consumer.text-bundle.retained-refused").unwrap(),
                SafeSummary::new("The consumer refused the decoded product retention query")
                    .unwrap(),
            ))),
            2 => Ok(TextBundleDomain::runtime_limits().retained_bytes().get() + 1),
            3 => Ok(size_of::<TextBundle>() - 1),
            _ => panic!("the under-inline promise must not reach the retained-cost query"),
        }
    }

    fn contains_runtime_product(product: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(product, reference)
    }
}

#[test]
fn invalid_retained_costs_preserve_typed_errors_and_the_next_full_decode_reservation() {
    let fixture = Fixture::new();
    let base = fixture.open_for_one_asset();
    let inline = size_of::<TextBundle>();
    let promised = TextBundleDomain::runtime_limits().retained_bytes().get();
    for mode in 0..=4 {
        let result = match mode {
            0 => base.acquire_asset::<TextBundleDomain, InvalidRetainedCost<0>>(selected(FIRST)),
            1 => base.acquire_asset::<TextBundleDomain, InvalidRetainedCost<1>>(selected(FIRST)),
            2 => base.acquire_asset::<TextBundleDomain, InvalidRetainedCost<2>>(selected(FIRST)),
            3 => base.acquire_asset::<TextBundleDomain, InvalidRetainedCost<3>>(selected(FIRST)),
            4 => base.acquire_asset::<TextBundleDomain, InvalidRetainedCost<4>>(selected(FIRST)),
            _ => unreachable!(),
        };
        match (mode, result) {
            (0, Err(RuntimeContentError::CodecPanicked)) => {}
            (1, Err(RuntimeContentError::Asset(error))) => assert_eq!(
                error.diagnostic().code().as_str(),
                "consumer.text-bundle.retained-refused",
            ),
            (
                2,
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                    required,
                    limit,
                }),
            ) => {
                assert_eq!(required, promised + 1);
                assert_eq!(limit, promised);
            }
            (
                3 | 4,
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                    required,
                    limit,
                }),
            ) => {
                assert_eq!(required, inline);
                assert_eq!(limit, inline - 1);
            }
            (mode, result) => panic!("unexpected retained-cost mode {mode} result: {result:?}"),
        }
        let recovered = base
            .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST))
            .unwrap();
        assert_eq!(
            recovered.product().get(&selected(FIRST)).unwrap().text,
            "Hello world",
        );
    }
}

#[test]
fn wrong_codec_and_panicking_decode_or_membership_do_not_spend_the_next_decode_reservation() {
    let fixture = Fixture::new();
    let base = fixture.open_for_one_asset();
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, WrongCodec>(selected(FIRST)),
        Err(RuntimeContentError::CodecMismatch),
    ));
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, PanickingDecode>(selected(FIRST)),
        Err(RuntimeContentError::CodecPanicked),
    ));
    assert!(matches!(
        base.acquire_asset::<PanickingMembership, TextBundleDomain>(selected(FIRST)),
        Err(RuntimeContentError::CodecPanicked),
    ));
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, PanickingIdentity>(selected(FIRST)),
        Err(RuntimeContentError::CodecPanicked),
    ));
    let recovered = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST))
        .unwrap();
    assert_eq!(
        recovered.product().get(&selected(FIRST)).unwrap().text,
        "Hello world"
    );
}

#[test]
fn codec_work_promises_and_shared_retained_output_are_admitted_before_decode() {
    let fixture = Fixture::new();
    let work = TextBundleDomain::runtime_limits().work_bytes().get();
    let too_little_work = fixture
        .open(RuntimeContentLimits::default().with_work_bytes(bytes(work - 1)))
        .unwrap();
    assert!(matches!(
        too_little_work.acquire_asset::<TextBundleDomain, PanickingDecode>(selected(FIRST)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::WorkBytes,
            required,
            limit,
        }) if required == work && limit == work - 1,
    ));
    let base = fixture.open_for_one_asset();
    let first = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(FIRST))
        .unwrap();
    let shared = first.clone();
    assert!(std::ptr::eq(first.product(), shared.product()));
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        }),
    ));
    drop(first);
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        }),
    ));
    drop(shared);
    let next = base
        .acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND))
        .unwrap();
    assert_eq!(
        next.product().get(&selected(SECOND)).unwrap().text,
        "Goodbye world"
    );
}

#[test]
fn host_capture_caps_constrain_the_catalog_owner_before_larger_allocation_or_member_io() {
    let fixture = Fixture::new();
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default().with_work_bytes(ByteLimit::ONE)),
        Err(RuntimeContentError::Catalog(
            nara::runtime_content::RuntimeContentCatalogError::BudgetExceeded {
                budget: "working-bytes",
                ..
            }
        )),
    ));
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default().with_member_encoded_bytes(ByteLimit::ONE)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::MemberEncodedBytes,
            ..
        }),
    ));
    // A missing catalog must not mask an already impossible raw-input reservation.
    fs::remove_file(fixture.root.join(RUNTIME_CONTENT_CATALOG_FILE)).unwrap();
    assert!(matches!(
        fixture.open(RuntimeContentLimits::default().with_retained_bytes(ByteLimit::ONE)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        }),
    ));
}

#[test]
fn catalog_reference_inventory_uses_the_admitted_scene_value_limits() {
    let mut fixture = Fixture::new();
    let plan = resolve_runtime_configuration(
        &fixture.catalog.configuration().resolve().unwrap(),
        recipe(),
    )
    .unwrap();
    let encoded = scene_bytes_with_samples(
        plan.schema_validation().bindings(),
        STARTUP_SOURCE,
        selected(FIRST),
        vec![7; 20_000],
    );
    fs::write(fixture.root.join(STARTUP_FILE), &encoded).unwrap();
    fixture.rewrite_catalog(|definition| {
        let member = definition
            .members
            .iter_mut()
            .find(|member| member.path() == STARTUP_FILE)
            .unwrap();
        *member = RuntimeContentMember::new(
            member.path(),
            ContentDigest::of_bytes(&encoded),
            member.kind().clone(),
            member.products().to_vec(),
            member.dependencies().to_vec(),
        )
        .unwrap();
    });
    let base = fixture.open(RuntimeContentLimits::default()).unwrap();
    let scene = base.acquire_startup_scene().unwrap();
    let samples = scene.candidate().document().entities[0].components
        [&ComponentTypeId::new(CAPTION)]
        .value
        .get("samples")
        .unwrap();
    assert!(
        matches!(samples, nara::reflect::ComponentValue::List(values) if values.len() == 20_000)
    );
    fixture.rewrite_catalog(|definition| {
        let member = definition
            .members
            .iter_mut()
            .find(|member| member.path() == STARTUP_FILE)
            .unwrap();
        *member = RuntimeContentMember::new(
            member.path(),
            member.digest(),
            member.kind().clone(),
            member.products().to_vec(),
            vec![],
        )
        .unwrap();
    });
    let missing_edge = fixture.open(RuntimeContentLimits::default()).unwrap();
    assert!(matches!(
        missing_edge.acquire_startup_scene(),
        Err(RuntimeContentError::UndeclaredDependency)
    ));
}

static DECODE_GATE: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> = Mutex::new(None);

struct HeldDecode;

impl RuntimeAssetCodec<TextBundle> for HeldDecode {
    const RUNTIME_KIND: &'static str = TextBundleDomain::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TextBundleDomain::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TextBundleDomain::runtime_limits()
    }
    fn encode_runtime(product: &TextBundle) -> Result<Box<[u8]>, AssetRuntimeError> {
        TextBundleDomain::encode_runtime(product)
    }
    fn decode_runtime(encoded: &[u8]) -> Result<TextBundle, AssetRuntimeError> {
        let (started, release) = DECODE_GATE.lock().unwrap().take().unwrap();
        started.send(()).unwrap();
        release.recv_timeout(Duration::from_secs(5)).unwrap();
        TextBundleDomain::decode_runtime(encoded)
    }
    fn runtime_retained_bytes(product: &TextBundle) -> Result<usize, AssetRuntimeError> {
        TextBundleDomain::runtime_retained_bytes(product)
    }
    fn contains_runtime_product(product: &TextBundle, reference: AssetProductRef) -> bool {
        TextBundleDomain::contains_runtime_product(product, reference)
    }
}

#[test]
fn overlapping_decodes_share_admission_and_release_after_the_last_output_owner() {
    let fixture = Fixture::new();
    let base = fixture.open_for_one_asset();
    let (started, observe) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    *DECODE_GATE.lock().unwrap() = Some((started, blocked));
    let output = std::thread::scope(|scope| {
        let held =
            scope.spawn(|| base.acquire_asset::<TextBundleDomain, HeldDecode>(selected(FIRST)));
        observe.recv_timeout(Duration::from_secs(5)).unwrap();
        let refused = base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND));
        release.send(()).unwrap();
        let first = held.join().unwrap().unwrap();
        assert!(matches!(
            refused,
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                ..
            })
        ));
        first
    });
    assert!(matches!(
        base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND)),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        }),
    ));
    drop(output);
    assert!(
        base.acquire_asset::<TextBundleDomain, TextBundleDomain>(selected(SECOND))
            .is_ok()
    );
}

#[test]
fn current_scene_retains_its_decoded_budget_until_physical_runtime_retirement() {
    let fixture = Fixture::new();
    let limits = RuntimeContentLimits::default()
        .with_retained_bytes(RuntimeSceneLimits::default().working_bytes());
    let probe = fixture.open(limits).unwrap();
    let required = match probe.acquire_startup_scene() {
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            ..
        }) => required,
        other => panic!("expected the full predecode promise plus captured base: {other:?}"),
    };
    drop(probe);
    let base = fixture
        .open(limits.with_retained_bytes(bytes(required)))
        .unwrap();
    let startup = base.acquire_startup_scene().unwrap();
    let mut candidate = base
        .plan()
        .plugin_plan()
        .instantiate_runtime_candidate(
            RuntimeAdmissionReservation::try_acquire().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    let observed = Arc::new(Mutex::new(None));
    let observation = Arc::clone(&observed);
    let result = candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut World| {
                assert!(
                    !materialize_startup_scene(world, startup.scene_source())
                        .unwrap()
                        .has_errors()
                );
                *observation.lock().unwrap() = Some(());
            });
        })
        .unwrap();
    assert!(result.post_operation_fault.is_none());
    assert!(observed.lock().unwrap().is_some());
    let runtime = candidate.complete_startup().unwrap().promote();
    let view = CurrentScene::get(runtime.world()).unwrap().source_view();
    assert!(view.with_runtime_scene(|_| ()).is_some());
    assert!(matches!(
        base.acquire_startup_scene(),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        }),
    ));
    runtime.finish().unwrap();
    assert!(view.with_document(|_| ()).is_none());
    assert!(base.acquire_startup_scene().is_ok());
}
