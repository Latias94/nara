use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    RuntimeControl, RuntimeControlRequestResult, RuntimeInstance,
};
use nara_asset::{
    AssetDomain, AssetPathScope, AssetProductRef, AssetReferenceDemand, AssetRuntimeError,
    AssetRuntimeLimits, ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_fs::ContentDigest;
use nara_identity::SceneEntityId;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapAsset, ActionMapBuilder,
    ActionMapDomain, ActionMapLimits, ActionMapRuntimeCodec, InputBinding, InputPlugin, KeyCode,
    SemanticActions,
};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_scene::{
    ExpandedSceneCandidate, RuntimeSceneCandidate, RuntimeSceneLimits, SceneDocument,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

use crate::{
    ProductRecipe,
    project_host::{
        CompositionError, RuntimeConfigurationPlanError, RuntimePlanError,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentBudgetKind, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogError, RuntimeContentCatalogLimits, RuntimeContentError,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};

const INPUT_FILE: &str = "input.runtime.json";
const SCENE_FILE: &str = "startup.runtime.json";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn input_product() -> AssetProductRef {
    product("7c210000-0000-4000-8000-000000000001")
}

fn startup_product() -> AssetProductRef {
    product("7c210000-0000-4000-8000-000000000002")
}

fn asset(reference: AssetProductRef) -> ActionMapAsset {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .declare_context(ActionContext::new("menu").unwrap(), false)
        .unwrap();
    builder
        .bind(ActionBinding::key(
            ActionId::new("jump").unwrap(),
            KeyCode::Space,
        ))
        .unwrap();
    ActionMapAsset::new(reference, builder.compile().unwrap()).unwrap()
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("nara.test.processed-input.owner", 1),
        )
        .unwrap()
}

fn configuration() -> RuntimeConfiguration {
    let mut configuration = RuntimeConfiguration::default();
    configuration.runtime.preset = RuntimePreset::LocalHeadless;
    configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
    configuration.input.action_map = Some(input_product());
    configuration.tasks.io.workers = 1;
    configuration.tasks.compute.workers = 1;
    configuration.tasks.async_compute.workers = 1;
    configuration
}

fn input_member(
    encoded: &[u8],
    kind: RuntimeContentMemberKind,
    products: Vec<AssetProductRef>,
) -> RuntimeContentMember {
    RuntimeContentMember::new(
        INPUT_FILE,
        ContentDigest::of_bytes(encoded),
        kind,
        products,
        vec![],
    )
    .unwrap()
}

fn input_kind(version: u32) -> RuntimeContentMemberKind {
    RuntimeContentMemberKind::Asset {
        format: FormatKind::new(ActionMapRuntimeCodec::RUNTIME_KIND).unwrap(),
        version: FormatVersion::new(version).unwrap(),
    }
}

struct Fixture {
    root: PathBuf,
    recipe: ProductRecipe,
    expected: RuntimeContentExpectation,
    catalog: RuntimeContentCatalog,
}

impl Fixture {
    fn new(recipe: ProductRecipe, input: &[u8]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "nara_processed_input_owner_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let configuration = configuration();
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let source =
            SourceDocumentKey::stable_asset(startup_product().source(), SourceDocumentKind::Scene);
        let scene = ExpandedSceneCandidate::plain(
            source.clone(),
            Arc::new(SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("startup").unwrap(),
            )])),
        );
        let encoded = RuntimeSceneCandidate::cook(
            &scene,
            plan.schema_validation().bindings(),
            ProjectAssetDatabase::default().resolver(&AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(root.join(SCENE_FILE), &encoded).unwrap();
        fs::write(root.join(INPUT_FILE), input).unwrap();
        let target = RuntimeContentTarget::new("test-host", "processed-input-owner").unwrap();
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
                startup_scene: startup_product(),
                scenes: vec![startup_product()],
                addressable_roots: vec![],
                members: vec![
                    RuntimeContentMember::new(
                        SCENE_FILE,
                        ContentDigest::of_bytes(&encoded),
                        RuntimeContentMemberKind::Scene { source },
                        vec![startup_product()],
                        vec![],
                    )
                    .unwrap(),
                    input_member(input, input_kind(1), vec![input_product()]),
                ],
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            recipe,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
            catalog,
        }
    }

    fn valid() -> Self {
        Self::new(
            recipe(),
            &ActionMapRuntimeCodec::encode_runtime(&asset(input_product())).unwrap(),
        )
    }

    fn open(&self, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            self.recipe.clone(),
            limits,
        )
        .unwrap()
    }

    fn definition(&self) -> RuntimeContentCatalogDefinition {
        RuntimeContentCatalogDefinition {
            target: self.catalog.target().clone(),
            configuration: self.catalog.configuration().clone(),
            plugin_plan_fingerprint: self.catalog.plugin_plan_fingerprint(),
            schema_fingerprint: self.catalog.schema_fingerprint(),
            runtime_codecs_fingerprint: self.catalog.runtime_codecs_fingerprint(),
            startup_scene: self.catalog.startup_scene(),
            scenes: self.catalog.scenes().to_vec(),
            addressable_roots: self.catalog.addressable_roots().to_vec(),
            members: self.catalog.members().to_vec(),
        }
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
                .starts_with("nara_processed_input_owner_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

// An oversized reservation observes the real shared charge without allocating or retaining it.
fn retained_charge(base: &RuntimeContentBase) -> usize {
    let probe = RuntimeContentLimits::default().retained_bytes().get();
    match base.reserve_retained(probe).err().unwrap() {
        RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            ..
        } => required - probe,
        other => panic!("unexpected retention probe failure: {other:?}"),
    }
}

fn assert_refunded(base: &RuntimeContentBase, retained: usize) {
    assert_eq!(retained_charge(base), retained);
    let work = base
        .inner
        .budget
        .reserve(0, base.inner.limits.work_bytes().get())
        .unwrap();
    drop(work);
}

#[test]
fn escaped_bindings_retain_the_actual_acquisition_and_refund_only_on_last_drop() {
    let fixture = Fixture::valid();
    let base = fixture.open(RuntimeContentLimits::default());
    let baseline = retained_charge(&base);
    let content = base.acquire_startup_input().unwrap().unwrap();
    assert_eq!(content.product(), input_product());
    assert!(base.same_base(content.base()));
    assert!(content.shares_owner_with(&content.clone()));
    let acquisition = Arc::downgrade(&content.inner.asset.inner);
    let charged = retained_charge(&base);
    assert!(charged > baseline);
    let bindings = content.bindings();
    let last = bindings.clone();
    assert!(bindings.shares_owner_with(&last));
    assert!(std::ptr::eq(
        content.definition().bindings(),
        bindings.definition().bindings(),
    ));
    drop(content);
    drop(bindings);
    assert!(acquisition.upgrade().is_some());
    assert_eq!(retained_charge(&base), charged);
    assert_eq!(last.definition().bindings()[0].action.as_str(), "jump");
    drop(last);
    assert!(acquisition.upgrade().is_none());
    assert_refunded(&base, baseline);
}

#[test]
fn last_escaped_bindings_keep_the_original_base_after_files_and_other_owners_are_gone() {
    let fixture = Fixture::valid();
    let base = fixture.open(RuntimeContentLimits::default());
    let weak_base = Arc::downgrade(&base.inner);
    let content = base.acquire_startup_input().unwrap().unwrap();
    let bindings = content.bindings();
    drop(content);
    drop(base);
    drop(fixture);
    assert!(weak_base.upgrade().is_some());
    assert_eq!(
        bindings.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Space),
    );
    drop(bindings);
    assert!(weak_base.upgrade().is_none());
}

#[test]
fn independent_states_require_their_own_exact_allowance_and_keep_contexts_independent() {
    let fixture = Fixture::valid();
    let base = fixture.open(RuntimeContentLimits::default());
    let content = base.acquire_startup_input().unwrap().unwrap();
    let baseline = retained_charge(&base);
    let required = ActionMap::required_state_bytes();
    let room = RuntimeContentLimits::default().retained_bytes().get() - baseline;
    let pressure = base.reserve_retained(room - (required - 1)).unwrap();
    let before = retained_charge(&base);
    assert!(matches!(
        content.prepare_state(),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        })
    ));
    assert_refunded(&base, before);
    drop(pressure);

    let pressure = base.reserve_retained(room - required).unwrap();
    let mut first = content.prepare_state().unwrap();
    assert!(base.reserve_retained(1).is_err());
    drop(pressure);
    assert_eq!(retained_charge(&base), baseline + required);
    first.disable_context(&ActionContext::gameplay()).unwrap();
    let second = content.prepare_state().unwrap();
    assert_eq!(retained_charge(&base), baseline + 2 * required);
    assert!(!first.is_context_enabled(&ActionContext::gameplay()));
    assert!(second.is_context_enabled(&ActionContext::gameplay()));
    assert!(
        first
            .bindings_handle()
            .shares_owner_with(&second.bindings_handle())
    );
    drop(first);
    assert_eq!(retained_charge(&base), baseline + required);
    drop(second);
    assert_refunded(&base, baseline);
}

#[derive(Default)]
struct InputChanges {
    press: bool,
    replace: bool,
}

#[derive(Clone, Default, Resource)]
struct InputControls(Arc<Mutex<InputChanges>>);

fn change_input(
    controls: Res<InputControls>,
    mut map: ResMut<ActionMap>,
    mut timeline: ResMut<nara_input::InputTimeline>,
) {
    let mut changes = controls.0.lock().unwrap();
    if std::mem::take(&mut changes.replace) {
        *map = ActionMap::default();
    }
    if std::mem::take(&mut changes.press) {
        timeline
            .admit_keyboard(
                nara_input::InputWindowId::UNSPECIFIED,
                nara_input::InputSeatId::PRIMARY,
                nara_input::ButtonDriverInput::Press(KeyCode::Space),
            )
            .unwrap();
    }
}

#[test]
fn paused_queued_input_retains_only_its_state_charge_after_the_original_map_is_replaced() {
    let fixture = Fixture::valid();
    let base = fixture.open(RuntimeContentLimits::default());
    let baseline = retained_charge(&base);
    let content = base.acquire_startup_input().unwrap().unwrap();
    let acquisition = Arc::downgrade(&content.inner.asset.inner);
    let controls = InputControls::default();
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(content.prepare_state().unwrap())
        .unwrap();
    app.insert_resource(controls.clone()).unwrap();
    app.add_systems(
        CoreStage::First,
        change_input.before(nara_input::__private::InputSet::StageActions),
    )
    .unwrap();
    drop(content);
    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    controls.0.lock().unwrap().press = true;
    runtime.drive(Duration::ZERO).unwrap();
    controls.0.lock().unwrap().replace = true;
    runtime.drive(Duration::ZERO).unwrap();
    assert!(acquisition.upgrade().is_none());
    assert_eq!(
        retained_charge(&base),
        baseline + ActionMap::required_state_bytes(),
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        !runtime
            .world()
            .resource::<SemanticActions>()
            .pressed(&ActionId::new("jump").unwrap(), &ActionContext::gameplay(),)
    );
    assert_refunded(&base, baseline);
    runtime.finish().unwrap();
}

#[test]
fn failed_decode_identity_and_encoded_limit_refund_the_owner_and_decode_reservations() {
    let foreign = product("7c210000-0000-4000-8000-000000000003");
    let cases = [
        (b"{".to_vec(), "decode"),
        (
            ActionMapRuntimeCodec::encode_runtime(&asset(foreign))
                .unwrap()
                .into_vec(),
            "identity",
        ),
        (
            vec![
                b' ';
                ActionMapRuntimeCodec::runtime_limits()
                    .encoded_bytes()
                    .get()
                    + 1
            ],
            "encoded",
        ),
    ];
    for (encoded, case) in cases {
        let fixture = Fixture::new(recipe(), &encoded);
        let base = fixture.open(RuntimeContentLimits::default());
        let baseline = retained_charge(&base);
        let error = base.acquire_startup_input().unwrap_err();
        assert!(match (case, error) {
            ("decode", RuntimeContentError::Asset(_))
            | ("identity", RuntimeContentError::ProductIdentityMismatch)
            | (
                "encoded",
                RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::MemberEncodedBytes,
                    ..
                },
            ) => true,
            (_, error) => panic!("unexpected {case} rejection: {error:?}"),
        });
        assert_refunded(&base, baseline);
    }
}

#[test]
fn refused_work_and_retained_promises_refund_partial_input_metadata_and_allow_retry() {
    let fixture = Fixture::valid();
    let work = ActionMapRuntimeCodec::runtime_limits().work_bytes().get();
    let base = fixture
        .open(RuntimeContentLimits::default().with_work_bytes(ByteLimit::new(work - 1).unwrap()));
    let baseline = retained_charge(&base);
    assert!(matches!(
        base.acquire_startup_input(),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::WorkBytes,
            ..
        })
    ));
    assert_refunded(&base, baseline);

    let base = fixture.open(RuntimeContentLimits::default());
    let baseline = retained_charge(&base);
    let room = RuntimeContentLimits::default().retained_bytes().get() - baseline;
    let pressure = base.reserve_retained(room - 16 * 1024).unwrap();
    let before = retained_charge(&base);
    assert!(matches!(
        base.acquire_startup_input(),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        })
    ));
    assert_refunded(&base, before);
    drop(pressure);
    let content = base.acquire_startup_input().unwrap().unwrap();
    assert_eq!(content.product(), input_product());
    drop(content);
    assert_refunded(&base, baseline);
}

#[test]
fn catalog_requires_a_primary_single_product_member_with_the_expected_format_and_version() {
    let fixture = Fixture::valid();
    let mut missing = fixture.definition();
    missing
        .members
        .retain(|member| !member.products().contains(&input_product()));
    let mut auxiliary = fixture.definition();
    auxiliary.configuration.input.action_map = Some(
        AssetProductRef::new(
            input_product().source(),
            ImportedProductId::parse_subproduct("7c210000-0000-4000-8000-000000000004").unwrap(),
        )
        .unwrap(),
    );
    let mut scene = fixture.definition();
    scene.configuration.input.action_map = Some(startup_product());
    let mut cases = vec![missing, auxiliary, scene];
    for (kind, products) in [
        (
            RuntimeContentMemberKind::Asset {
                format: FormatKind::new("other_action_map").unwrap(),
                version: FormatVersion::ONE,
            },
            vec![input_product()],
        ),
        (input_kind(2), vec![input_product()]),
        (
            input_kind(1),
            vec![
                input_product(),
                product("7c210000-0000-4000-8000-000000000005"),
            ],
        ),
    ] {
        let mut definition = fixture.definition();
        let member = definition
            .members
            .iter_mut()
            .find(|member| member.products().contains(&input_product()))
            .unwrap();
        *member = input_member(b"metadata-only", kind, products);
        cases.push(definition);
    }
    for definition in cases {
        assert!(matches!(
            RuntimeContentCatalog::new(definition, RuntimeContentCatalogLimits::default()),
            Err(RuntimeContentCatalogError::InvalidStartupInput { .. })
        ));
    }
    let decoded = RuntimeContentCatalog::from_json_bytes(
        &fixture.catalog.to_json_bytes().unwrap(),
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    assert_eq!(
        decoded.configuration().input.action_map,
        Some(input_product()),
    );
    assert!(decoded.addressable_roots().is_empty());
}

struct OtherActionMapDomain;

impl AssetDomain for OtherActionMapDomain {
    type Product = ActionMapAsset;
}

static PROBE_CONSTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

struct ConstructionProbe;

impl Default for ConstructionProbe {
    fn default() -> Self {
        PROBE_CONSTRUCTIONS.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Plugin for ConstructionProbe {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.processed-input.construction"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        panic!("Input codec admission must precede App construction")
    }
}

#[test]
fn missing_codec_and_same_format_wrong_family_refuse_before_any_plugin_is_constructed() {
    let before = PROBE_CONSTRUCTIONS.load(Ordering::Relaxed);
    for recipe in [
        ProductRecipe::new(),
        ProductRecipe::new()
            .add_runtime_asset_codec::<OtherActionMapDomain, ActionMapRuntimeCodec>(
                RuntimeCodecDefinitionId::new("nara.test.processed-input.wrong-family", 1),
            )
            .unwrap(),
    ] {
        let error = resolve_runtime_configuration(
            &configuration().resolve().unwrap(),
            recipe.add_plugin::<ConstructionProbe>().unwrap(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            RuntimeConfigurationPlanError::Resolution(RuntimePlanError::Composition(
                CompositionError::InputCodecRequired,
            ))
        ));
        assert_eq!(PROBE_CONSTRUCTIONS.load(Ordering::Relaxed), before);
    }
}

struct SelectedCodec;
static SELECTED_ENCODED_LIMIT: AtomicUsize = AtomicUsize::new(256 * 1024);
static SELECTED_DECODES: AtomicUsize = AtomicUsize::new(0);

impl RuntimeAssetCodec<ActionMapAsset> for SelectedCodec {
    const RUNTIME_KIND: &'static str = ActionMapRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = ActionMapRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        let limits = ActionMapRuntimeCodec::runtime_limits();
        AssetRuntimeLimits::new(
            ByteLimit::new(SELECTED_ENCODED_LIMIT.load(Ordering::Relaxed)).unwrap(),
            limits.work_bytes(),
            limits.retained_bytes(),
        )
    }

    fn encode_runtime(product: &ActionMapAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        ActionMapRuntimeCodec::encode_runtime(product)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<ActionMapAsset, AssetRuntimeError> {
        SELECTED_DECODES.fetch_add(1, Ordering::Relaxed);
        ActionMapRuntimeCodec::decode_runtime(bytes)
    }

    fn runtime_retained_bytes(product: &ActionMapAsset) -> Result<usize, AssetRuntimeError> {
        ActionMapRuntimeCodec::runtime_retained_bytes(product)
    }

    fn visit_runtime_dependencies<E>(
        product: &ActionMapAsset,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        ActionMapRuntimeCodec::visit_runtime_dependencies(product, visitor)
    }

    fn contains_runtime_product(product: &ActionMapAsset, reference: AssetProductRef) -> bool {
        ActionMapRuntimeCodec::contains_runtime_product(product, reference)
    }
}

#[test]
fn startup_input_executes_the_actual_selected_alternative_codec_with_frozen_limits() {
    let recipe = ProductRecipe::new()
        .add_runtime_asset_codec::<ActionMapDomain, SelectedCodec>(RuntimeCodecDefinitionId::new(
            "nara.test.processed-input.selected-alternative",
            1,
        ))
        .unwrap();
    let fixture = Fixture::new(
        recipe,
        &ActionMapRuntimeCodec::encode_runtime(&asset(input_product())).unwrap(),
    );
    let base = fixture.open(RuntimeContentLimits::default());
    SELECTED_ENCODED_LIMIT.store(1, Ordering::Relaxed);
    let before = SELECTED_DECODES.load(Ordering::Relaxed);
    let baseline = retained_charge(&base);
    let content = base.acquire_startup_input().unwrap().unwrap();
    assert_eq!(SELECTED_DECODES.load(Ordering::Relaxed), before + 1);
    assert_eq!(content.definition().bindings()[0].action.as_str(), "jump");
    assert!(matches!(
        base.acquire_asset::<ActionMapDomain, ActionMapRuntimeCodec>(input_product()),
        Err(RuntimeContentError::CodecNotSelected)
    ));
    drop(content);
    assert_refunded(&base, baseline);
}
