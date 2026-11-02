use std::{
    cell::{Cell, RefCell},
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{
    AssetDomain, AssetPathScope, AssetRecord, AssetReferenceDemand, AssetRuntimeError,
    AssetRuntimeLimits, AssetSourceKind, AssetSourcePath, ImportedProductId, ProjectAssetDatabase,
    RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_diagnostic::{Diagnostic, DiagnosticCode, SafeSummary};
use nara_fs::{ContentDigest, RelativePath};
use nara_identity::SceneEntityId;
use nara_image::{ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue, ComponentValueShape, PersistentComponentSchemaOwnerId,
    encode_asset_reference_value,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};
use serde::{Deserialize, Serialize};

use super::*;
use crate::{
    ProductRecipe, SchemaContribution,
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentExpectation, RuntimeContentLimits,
        RuntimeContentMember, RuntimeContentTarget, native::ImageFact,
    },
};

const X: u8 = 1;
const Y: u8 = 2;
const Z: u8 = 3;
const IMAGE: u8 = 4;
const SCENE_WORK: usize = 1024 * 1024;
const Y_WORK: usize = 4 * 1024 * 1024;
const Y_OUTPUT: usize = 2 * 1024 * 1024;
const COMPONENT: &str = "nara.test.PreparedReferences";
const FIXTURE_PREFIX: &str = "nara-prepared-discovery-";
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Observations {
    decoded: Vec<(u8, usize)>,
    dropped: Vec<(u8, usize)>,
    image_decodes: usize,
}

// The production acquisition and collection assembly are synchronous. Thread-local observations
// isolate these fixtures from other tests without serializing unrelated Runtime codecs.
thread_local! {
    static OBSERVED: RefCell<Observations> = RefCell::new(Observations::default());
    static REFUSE_Y: Cell<bool> = const { Cell::new(false) };
    static PANIC_Y: Cell<bool> = const { Cell::new(false) };
    static REQUIRE_PREVIOUS_DROP: Cell<bool> = const { Cell::new(false) };
}

fn reset_observations() {
    OBSERVED.with(|observed| *observed.borrow_mut() = Observations::default());
    REFUSE_Y.with(|refuse| refuse.set(false));
    PANIC_Y.with(|panic| panic.set(false));
    REQUIRE_PREVIOUS_DROP.with(|require| require.set(false));
}

fn decoded() -> Vec<u8> {
    OBSERVED.with(|observed| {
        observed
            .borrow()
            .decoded
            .iter()
            .map(|(tag, _)| *tag)
            .collect()
    })
}

fn dropped() -> Vec<u8> {
    let mut tags = OBSERVED.with(|observed| {
        observed
            .borrow()
            .dropped
            .iter()
            .map(|(tag, _)| *tag)
            .collect::<Vec<_>>()
    });
    tags.sort_unstable();
    tags
}

fn product(tag: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(&format!("7c230000-0000-4000-8000-{tag:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn owner() -> PersistentComponentSchemaOwnerId {
    PersistentComponentSchemaOwnerId::try_new("nara.test.prepared-discovery").unwrap()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireProduct {
    reference: AssetProductRef,
    references: Vec<(AssetProductRef, AssetReferenceDemand)>,
}

#[derive(Debug)]
struct ProbeProduct {
    tag: u8,
    wire: WireProduct,
    payload: Box<[u8]>,
    instance: usize,
}

impl ProbeProduct {
    fn fixture(tag: u8) -> Self {
        let references = match tag {
            X => vec![
                (product(Y), AssetReferenceDemand::Deferred),
                (product(Y), AssetReferenceDemand::Immediate),
            ],
            Y => vec![(product(Z), AssetReferenceDemand::Immediate)],
            Z => vec![(product(X), AssetReferenceDemand::Immediate)],
            _ => unreachable!(),
        };
        Self {
            tag,
            wire: WireProduct {
                reference: product(tag),
                references,
            },
            payload: Vec::new().into_boxed_slice(),
            instance: 0,
        }
    }
}

impl Drop for ProbeProduct {
    fn drop(&mut self) {
        if self.instance != 0 {
            OBSERVED.with(|observed| {
                observed
                    .borrow_mut()
                    .dropped
                    .push((self.tag, self.instance));
            });
        }
    }
}

struct ProbeDomain;
impl AssetDomain for ProbeDomain {
    type Product = ProbeProduct;
}

fn codec_error() -> AssetRuntimeError {
    AssetRuntimeError::new(Diagnostic::error(
        DiagnosticCode::new("test.prepared-codec-refused").unwrap(),
        SafeSummary::new("The prepared-content fixture codec refused its input").unwrap(),
    ))
}

fn decode_product(tag: u8, bytes: &[u8]) -> Result<ProbeProduct, AssetRuntimeError> {
    if REQUIRE_PREVIOUS_DROP.with(Cell::get) {
        // This runs inside the next actual heavy decoder, before it allocates its output.
        // A retained validation closure or a second discovery decode fails the oracle here.
        match tag {
            Y => assert_eq!(dropped(), [X]),
            Z => assert_eq!(dropped(), [X, Y]),
            _ => {}
        }
    }
    let wire: WireProduct = serde_json::from_slice(bytes).map_err(|_| codec_error())?;
    if wire.reference != product(tag) || wire.references.len() > 2 {
        return Err(codec_error());
    }
    let instance = OBSERVED.with(|observed| {
        let mut observed = observed.borrow_mut();
        let instance = observed.decoded.len() + 1;
        observed.decoded.push((tag, instance));
        instance
    });
    let payload_bytes = match tag {
        X => 64 * 1024,
        Y => 128 * 1024,
        Z => 32 * 1024,
        _ => unreachable!(),
    };
    let value = ProbeProduct {
        tag,
        wire,
        payload: vec![tag; payload_bytes].into_boxed_slice(),
        instance,
    };
    assert!(
        tag != Y || !PANIC_Y.with(Cell::get),
        "fixture decoder panic after allocating its product"
    );
    if tag == Y && REFUSE_Y.with(Cell::get) {
        return Err(codec_error());
    }
    Ok(value)
}

struct XCodec;
struct YCodec;
struct ZCodec;

macro_rules! codec {
    ($codec:ty, $tag:expr, $kind:literal, $work:expr, $output:expr) => {
        impl RuntimeAssetCodec<ProbeProduct> for $codec {
            const RUNTIME_KIND: &'static str = $kind;
            const RUNTIME_VERSION: u32 = 1;

            fn runtime_limits() -> AssetRuntimeLimits {
                AssetRuntimeLimits::new(
                    ByteLimit::new(4096).unwrap(),
                    ByteLimit::new($work).unwrap(),
                    ByteLimit::new($output).unwrap(),
                )
            }

            fn encode_runtime(value: &ProbeProduct) -> Result<Box<[u8]>, AssetRuntimeError> {
                serde_json::to_vec(&value.wire)
                    .map(Vec::into_boxed_slice)
                    .map_err(|_| codec_error())
            }

            fn decode_runtime(bytes: &[u8]) -> Result<ProbeProduct, AssetRuntimeError> {
                decode_product($tag, bytes)
            }

            fn runtime_retained_bytes(value: &ProbeProduct) -> Result<usize, AssetRuntimeError> {
                Ok(size_of::<ProbeProduct>()
                    + value.payload.len()
                    + value.wire.references.capacity()
                        * size_of::<(AssetProductRef, AssetReferenceDemand)>())
            }

            fn visit_runtime_dependencies<E>(
                value: &ProbeProduct,
                mut visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
            ) -> Result<(), E> {
                for &(reference, demand) in &value.wire.references {
                    visitor(reference, demand)?;
                }
                Ok(())
            }

            fn contains_runtime_product(value: &ProbeProduct, reference: AssetProductRef) -> bool {
                value.wire.reference == reference
            }
        }
    };
}

codec!(XCodec, X, "prepared_x", 256 * 1024, 256 * 1024);
codec!(YCodec, Y, "prepared_y", Y_WORK, Y_OUTPUT);
codec!(ZCodec, Z, "prepared_z", 512 * 1024, 256 * 1024);

struct SmallImageCodec;
impl RuntimeAssetCodec<ImageAsset> for SmallImageCodec {
    const RUNTIME_KIND: &'static str = ImageRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = ImageRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        // This concrete fixture format admits only one pixel and at most 4096 encoded bytes.
        // Its original promise therefore stays below the heavy Y decoder's selected bounds.
        AssetRuntimeLimits::new(
            ByteLimit::new(4096).unwrap(),
            ByteLimit::new(128 * 1024).unwrap(),
            ByteLimit::new(16 * 1024).unwrap(),
        )
    }

    fn encode_runtime(image: &ImageAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        ImageRuntimeCodec::encode_runtime(image)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<ImageAsset, AssetRuntimeError> {
        let image = ImageRuntimeCodec::decode_runtime(bytes)?;
        if image.extent()
            != (ImageExtent {
                width: 1,
                height: 1,
            })
        {
            return Err(codec_error());
        }
        OBSERVED.with(|observed| observed.borrow_mut().image_decodes += 1);
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

#[derive(Default)]
struct FixturePlugin;
impl Plugin for FixturePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.prepared-discovery"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn contribution() -> SchemaContribution<FixturePlugin> {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(COMPONENT),
        "Prepared references",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities([ComponentCapability::Scene])
    .with_fields([
        ComponentFieldSchema::required(
            ComponentFieldId::new("now"),
            "Now",
            ComponentFieldPath::from_fields(["now"]),
            ComponentValueShape::list(ComponentValueShape::asset_ref(
                AssetReferenceDemand::Immediate,
            )),
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
        ComponentFieldSchema::optional_with_default(
            ComponentFieldId::new("future"),
            "Future",
            ComponentFieldPath::from_fields(["future"]),
            ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
            encode_asset_reference_value(&AssetRef::Product(product(X))),
        )
        .with_capabilities([ComponentCapability::Scene, ComponentCapability::AssetRef]),
    ]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        owner(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SchemaContribution::data_for_default([owner]).unwrap()
}

fn recipe(reversed: bool, contribution: SchemaContribution<FixturePlugin>) -> ProductRecipe {
    let recipe = ProductRecipe::new()
        .add_contribution(contribution)
        .unwrap()
        .add_runtime_image_codec::<SmallImageCodec>(RuntimeCodecDefinitionId::new(
            "nara.test.prepared.b",
            1,
        ))
        .unwrap();
    let x = RuntimeCodecDefinitionId::new("nara.test.prepared.z", 1);
    let y = RuntimeCodecDefinitionId::new("nara.test.prepared.a", 1);
    let z = RuntimeCodecDefinitionId::new("nara.test.prepared.zz", 1);
    if reversed {
        recipe
            .add_runtime_asset_codec::<ProbeDomain, ZCodec>(z)
            .unwrap()
            .add_runtime_asset_codec::<ProbeDomain, YCodec>(y)
            .unwrap()
            .add_runtime_asset_codec::<ProbeDomain, XCodec>(x)
            .unwrap()
    } else {
        recipe
            .add_runtime_asset_codec::<ProbeDomain, XCodec>(x)
            .unwrap()
            .add_runtime_asset_codec::<ProbeDomain, YCodec>(y)
            .unwrap()
            .add_runtime_asset_codec::<ProbeDomain, ZCodec>(z)
            .unwrap()
    }
}

fn scene_limits() -> RuntimeSceneLimits {
    RuntimeSceneLimits::default().with_working_bytes(ByteLimit::new(SCENE_WORK).unwrap())
}

fn content_limits() -> RuntimeContentLimits {
    RuntimeContentLimits::default().with_scene(scene_limits())
}

struct Fixture {
    root: PathBuf,
    recipes: [ProductRecipe; 2],
    expected: RuntimeContentExpectation,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_PREFIX}{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let contribution = contribution();
        let recipes = [
            recipe(false, contribution.clone()),
            recipe(true, contribution),
        ];
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), recipes[0].clone())
                .unwrap();
        let source =
            SourceDocumentKey::stable_asset(product(0).source(), SourceDocumentKind::Scene);
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("root").unwrap(),
        )
        .with_component(
            ComponentTypeId::new(COMPONENT),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::map([(
                    "now",
                    ComponentValue::List(vec![
                        encode_asset_reference_value(&AssetRef::Product(product(X))),
                        encode_asset_reference_value(&AssetRef::Product(product(Z))),
                        encode_asset_reference_value(&AssetRef::Product(product(IMAGE))),
                    ]),
                )]),
            )
            .with_owner(owner()),
        )]);
        let scene = crate::startup_scene::compile_test_scene(
            &document,
            plan.schema_validation().bindings(),
            source.clone(),
        );
        let mut database = ProjectAssetDatabase::default();
        for tag in [X, Y, Z, IMAGE] {
            database
                .insert(AssetRecord::new(
                    product(tag).source(),
                    AssetSourcePath::new(format!("assets/{tag}.bin")).unwrap(),
                    if tag == IMAGE {
                        AssetSourceKind::Image
                    } else {
                        AssetSourceKind::Unknown
                    },
                ))
                .unwrap();
        }
        let encoded = RuntimeSceneCandidate::cook(
            &scene,
            plan.schema_validation().bindings(),
            database.resolver(&AssetPathScope::ROOT),
            scene_limits(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(root.join("00-scene.json"), &encoded).unwrap();
        let required = |tag| {
            RuntimeContentDependency::new(product(tag), RuntimeContentDependencyRole::Required)
        };
        let mut members = vec![
            RuntimeContentMember::new(
                "00-scene.json",
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Scene { source },
                vec![product(0)],
                vec![required(X), required(Z), required(IMAGE)],
            )
            .unwrap(),
        ];
        for (tag, path, kind, target, encoded) in [
            (
                X,
                "20-x.json",
                XCodec::RUNTIME_KIND,
                Y,
                XCodec::encode_runtime(&ProbeProduct::fixture(X)).unwrap(),
            ),
            (
                Y,
                "10-y.json",
                YCodec::RUNTIME_KIND,
                Z,
                YCodec::encode_runtime(&ProbeProduct::fixture(Y)).unwrap(),
            ),
            (
                Z,
                "30-z.json",
                ZCodec::RUNTIME_KIND,
                X,
                ZCodec::encode_runtime(&ProbeProduct::fixture(Z)).unwrap(),
            ),
        ] {
            fs::write(root.join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(kind).unwrap(),
                        version: FormatVersion::new(1).unwrap(),
                    },
                    vec![product(tag)],
                    vec![required(target)],
                )
                .unwrap(),
            );
        }
        let image = ImageAsset::from_product(
            product(IMAGE),
            ImageExtent {
                width: 1,
                height: 1,
            },
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![11, 22, 33, 255],
        )
        .unwrap();
        let encoded = SmallImageCodec::encode_runtime(&image).unwrap();
        fs::write(root.join("05-image.bin"), &encoded).unwrap();
        members.push(
            RuntimeContentMember::new(
                "05-image.bin",
                ContentDigest::of_bytes(&encoded),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(SmallImageCodec::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::new(SmallImageCodec::RUNTIME_VERSION).unwrap(),
                },
                vec![product(IMAGE)],
                vec![],
            )
            .unwrap(),
        );
        let target = RuntimeContentTarget::new("test-host", "prepared-discovery").unwrap();
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
                startup_scene: product(0),
                scenes: vec![product(0)],
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
            recipes,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
        }
    }

    fn open(&self, reversed: bool, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(&self.root).unwrap(),
            &self.expected,
            self.recipes[usize::from(reversed)].clone(),
            limits,
        )
        .unwrap()
    }

    fn write_pack(&self) -> PathBuf {
        let path = self.root.join(format!(
            "content-{}.pack",
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        self.open(false, content_limits())
            .write_pack(&mut fs::File::create_new(&path).unwrap())
            .unwrap();
        path
    }

    fn open_pack(
        &self,
        path: &std::path::Path,
        limits: RuntimeContentLimits,
    ) -> Result<RuntimeContentBase, RuntimeContentError> {
        let directory = open_explicit_product_project_root(&self.root).unwrap();
        let file = directory
            .open_file(&RelativePath::new(path.file_name().unwrap().to_str().unwrap()).unwrap())
            .unwrap();
        RuntimeContentBase::open_pack(file, &self.expected, self.recipes[0].clone(), limits)
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
                .starts_with(FIXTURE_PREFIX)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

// Probes reserve no memory. A refusal exposes the live shared charge; a successful zero-charge
// probe returns its reservation immediately. This uses the real budget without private counters.
fn charge(base: &RuntimeContentBase, kind: RuntimeContentBudgetKind) -> usize {
    let (retained, work, probe) = match kind {
        RuntimeContentBudgetKind::RetainedBytes => {
            let limit = base.inner.limits.retained_bytes().get();
            (limit, 0, limit)
        }
        RuntimeContentBudgetKind::WorkBytes => {
            let limit = base.inner.limits.work_bytes().get();
            (0, limit, limit)
        }
        _ => unreachable!(),
    };
    match base.inner.budget.reserve(retained, work) {
        Ok(lease) => {
            drop(lease);
            0
        }
        Err(RuntimeContentError::Budget {
            kind: actual,
            required,
            limit,
        }) => {
            assert_eq!(actual, kind);
            assert_eq!(limit, probe);
            required - probe
        }
        Err(error) => panic!("unexpected budget probe: {error:?}"),
    }
}

fn assert_refunded(base: &RuntimeContentBase, baseline: usize) {
    assert_eq!(
        charge(base, RuntimeContentBudgetKind::RetainedBytes),
        baseline
    );
    assert_eq!(charge(base, RuntimeContentBudgetKind::WorkBytes), 0);
}

#[test]
fn retained_pack_releases_member_encoding_and_overlaps_exact_decode_work() {
    let fixture = Fixture::new();
    let path = fixture.write_pack();
    let captured = fixture.open(false, content_limits());
    let pack = fixture.open_pack(&path, content_limits()).unwrap();
    let captured_charge = charge(&captured, RuntimeContentBudgetKind::RetainedBytes);
    let pack_charge = charge(&pack, RuntimeContentBudgetKind::RetainedBytes);
    let member_bytes: usize = captured
        .catalog()
        .members()
        .iter()
        .map(|member| member.digest().length() as usize)
        .sum();
    assert_eq!(
        captured_charge - pack_charge,
        member_bytes
            + captured.catalog().members().len() * (size_of::<Vec<u8>>() - size_of::<u64>()),
    );
    let member = pack.member(product(Y)).unwrap();
    let exact = Y_WORK
        + member.dependencies().len() * size_of::<Option<AssetReferenceDemand>>()
        + member.digest().length() as usize;
    drop((captured, pack));
    reset_observations();
    for allowance in [exact - 1, exact] {
        let base = fixture
            .open_pack(
                &path,
                content_limits().with_work_bytes(ByteLimit::new(allowance).unwrap()),
            )
            .unwrap();
        let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
        if allowance < exact {
            assert!(matches!(
                base.acquire_asset::<ProbeDomain, YCodec>(product(Y)),
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::WorkBytes,
                    required,
                    limit,
                }) if required == exact && limit == allowance
            ));
            assert!(decoded().is_empty());
        } else {
            let first = base
                .acquire_asset::<ProbeDomain, YCodec>(product(Y))
                .unwrap();
            assert_eq!(charge(&base, RuntimeContentBudgetKind::WorkBytes), 0);
            assert_eq!(
                charge(&base, RuntimeContentBudgetKind::RetainedBytes),
                baseline + first.retained_bytes(),
            );
            let second = base
                .acquire_asset::<ProbeDomain, YCodec>(product(Y))
                .unwrap();
            assert_eq!(decoded(), [Y, Y]);
            assert_eq!(
                charge(&base, RuntimeContentBudgetKind::RetainedBytes),
                baseline + first.retained_bytes() + second.retained_bytes(),
            );
            drop((first, second));
        }
        assert_refunded(&base, baseline);
    }
}

#[test]
fn retained_pack_decode_failure_and_later_mutation_keep_refund_and_decoded_owners() {
    let fixture = Fixture::new();
    let path = fixture.write_pack();
    let base = fixture.open_pack(&path, content_limits()).unwrap();
    let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
    reset_observations();
    REFUSE_Y.with(|refuse| refuse.set(true));
    assert!(matches!(
        base.acquire_asset::<ProbeDomain, YCodec>(product(Y)),
        Err(RuntimeContentError::Asset(_))
    ));
    assert_eq!(decoded(), [Y]);
    assert_refunded(&base, baseline);
    REFUSE_Y.with(|refuse| refuse.set(false));
    PANIC_Y.with(|panic| panic.set(true));
    assert!(matches!(
        base.acquire_asset::<ProbeDomain, YCodec>(product(Y)),
        Err(RuntimeContentError::CodecPanicked)
    ));
    PANIC_Y.with(|panic| panic.set(false));
    assert_eq!(decoded(), [Y, Y]);
    assert_refunded(&base, baseline);
    let original = base
        .acquire_asset::<ProbeDomain, YCodec>(product(Y))
        .unwrap();
    let mut changed = fs::read(&path).unwrap();
    *changed.last_mut().unwrap() ^= 1;
    fs::write(&path, &changed).unwrap();
    let last = base.catalog().members().last().unwrap().products()[0];
    assert_eq!(last, product(Z));
    assert!(matches!(
        base.acquire_asset::<ProbeDomain, ZCodec>(last),
        Err(RuntimeContentError::MemberDigestMismatch)
    ));
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(changed.len() as u64 - 1)
        .unwrap();
    assert!(matches!(
        base.acquire_asset::<ProbeDomain, ZCodec>(last),
        Err(RuntimeContentError::Host(_))
    ));
    assert_eq!(decoded(), [Y, Y, Y]);
    assert_eq!(original.product().payload[0], Y);
    assert_eq!(charge(&base, RuntimeContentBudgetKind::WorkBytes), 0);
    assert_eq!(
        charge(&base, RuntimeContentBudgetKind::RetainedBytes),
        baseline + original.retained_bytes()
    );
    drop(original);
    assert_refunded(&base, baseline);
}

#[test]
fn retained_pack_rejects_bad_header_extent_and_catalog_before_domain_decode() {
    let fixture = Fixture::new();
    let path = fixture.write_pack();
    let original = fs::read(&path).unwrap();
    reset_observations();
    for byte in [0, 8, 12] {
        let mut changed = original.clone();
        changed[byte] ^= 1;
        fs::write(&path, changed).unwrap();
        assert!(matches!(
            fixture.open_pack(&path, content_limits()),
            Err(RuntimeContentError::InvalidPackHeader)
        ));
    }
    let mut changed = original.clone();
    changed[20] ^= 1;
    fs::write(&path, changed).unwrap();
    assert!(matches!(
        fixture.open_pack(&path, content_limits()),
        Err(RuntimeContentError::CatalogDigestMismatch)
    ));
    for length in [original.len() - 1, original.len() + 1] {
        let mut changed = original.clone();
        changed.resize(length, 0);
        fs::write(&path, changed).unwrap();
        assert!(matches!(
            fixture.open_pack(&path, content_limits()),
            Err(RuntimeContentError::PackExtentMismatch)
        ));
    }
    fs::write(&path, &original[..10]).unwrap();
    assert!(matches!(
        fixture.open_pack(&path, content_limits()),
        Err(RuntimeContentError::Host(_))
    ));
    assert!(decoded().is_empty());
}

#[test]
fn retained_pack_partial_sink_failure_releases_selected_raw_work_without_decoding() {
    struct RefusingSink {
        remaining: usize,
    }
    impl std::io::Write for RefusingSink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::other("fixture sink refused further bytes"));
            }
            let written = self.remaining.min(bytes.len());
            self.remaining -= written;
            Ok(written)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let fixture = Fixture::new();
    let path = fixture.write_pack();
    let captured = fixture.open(false, content_limits());
    let pack = fixture.open_pack(&path, content_limits()).unwrap();
    reset_observations();
    for base in [captured, pack] {
        let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
        for remaining in [3, 24, 20 + base.catalog_digest().length() as usize + 1] {
            let mut sink = RefusingSink { remaining };
            assert!(matches!(
                base.write_pack(&mut sink),
                Err(RuntimeContentError::Host(nara_fs::FsError::Io {
                    operation: nara_fs::FsOperation::Write,
                    ..
                }))
            ));
            assert_eq!(sink.remaining, 0);
            assert_refunded(&base, baseline);
            assert!(decoded().is_empty());
        }
    }
}

struct Peak {
    work: usize,
    retained: usize,
    deep_work: usize,
    deep_retained: usize,
}

fn discovery_peak(fixture: &Fixture) -> Peak {
    // Calibrate Host metadata through real acquisitions on a separate base. Decoder promises
    // are fixture constants. Observations are reset only after these setup owners are dropped.
    reset_observations();
    let base = fixture.open(false, content_limits());
    let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
    let graph = MemberTraversal::new(&base).unwrap();
    let graph_bytes = charge(&base, RuntimeContentBudgetKind::WorkBytes);
    drop(graph);
    let scene = base.acquire_startup_scene().unwrap();
    let image = base
        .acquire_asset::<ImageDomain, SmallImageCodec>(product(IMAGE))
        .unwrap();
    let x = base
        .acquire_asset::<ProbeDomain, XCodec>(product(X))
        .unwrap();
    let y = base
        .acquire_asset::<ProbeDomain, YCodec>(product(Y))
        .unwrap();
    let metadata = y.retained_bytes() - YCodec::runtime_retained_bytes(y.product()).unwrap();
    let peak = Peak {
        // At Y: graph + output slots + the live Image/X/Y erased wrappers + Y's original
        // work promise + Y's one dependency slot. Z is pending and has not decoded yet.
        work: graph_bytes
            + size_of::<(usize, RuntimeContentScene)>()
            + 4 * size_of::<DecodedAsset>()
            + size_of::<RuntimeContentAsset<ImageAsset>>()
            + 2 * size_of::<RuntimeContentAsset<ProbeProduct>>()
            + Y_WORK
            + size_of::<Option<AssetReferenceDemand>>(),
        retained: baseline
            + scene.retained_bytes()
            + image.retained_bytes()
            + x.retained_bytes()
            + Y_OUTPUT
            + metadata,
        // Strict validation keeps the compact Image fact, but has already disposed the Image,
        // Scene and X decoded outputs. Only Y's current erased wrapper remains as decode work.
        deep_work: graph_bytes
            + size_of::<(usize, ImageFact)>()
            + size_of::<RuntimeContentAsset<ProbeProduct>>()
            + Y_WORK
            + size_of::<Option<AssetReferenceDemand>>(),
        deep_retained: baseline + Y_OUTPUT + metadata,
    };
    drop((scene, image, x, y));
    assert_refunded(&base, baseline);
    reset_observations();
    peak
}

#[test]
fn retained_pack_complete_preparation_and_deep_validation_admit_raw_decode_overlap() {
    let fixture = Fixture::new();
    let path = fixture.write_pack();
    let peak = discovery_peak(&fixture);
    let base = fixture.open_pack(&path, content_limits()).unwrap();
    let y_encoded = base.member(product(Y)).unwrap().digest().length() as usize;
    drop(base);
    for deep in [false, true] {
        let exact = if deep { peak.deep_work } else { peak.work } + y_encoded;
        for short in [true, false] {
            let limit = exact - usize::from(short);
            let base = fixture
                .open_pack(
                    &path,
                    content_limits().with_work_bytes(ByteLimit::new(limit).unwrap()),
                )
                .unwrap();
            let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
            reset_observations();
            let result = if deep {
                REQUIRE_PREVIOUS_DROP.with(|require| require.set(true));
                base.validate_deep().map(|_| ())
            } else {
                base.prepare_scene_content(product(0)).map(|content| {
                    assert_prepared_once(&content);
                    drop(content);
                })
            };
            REQUIRE_PREVIOUS_DROP.with(|require| require.set(false));
            if short {
                assert!(matches!(
                    result,
                    Err(RuntimeContentError::Budget {
                        kind: RuntimeContentBudgetKind::WorkBytes, required, limit: actual
                    }) if required == exact && actual == limit
                ));
                assert_eq!(decoded(), [X]);
                assert_eq!(dropped(), [X]);
            } else {
                result.unwrap();
                assert_eq!(decoded(), [X, Y, Z]);
                assert_eq!(dropped(), [X, Y, Z]);
            }
            assert_refunded(&base, baseline);
        }
    }
}

fn assert_prepared_once(content: &ProcessedSceneContent) {
    assert_eq!(decoded(), [X, Y, Z]);
    assert!(dropped().is_empty());
    assert!(content.dependencies.is_empty());
    assert_eq!(content.images.len(), 1);
    OBSERVED.with(|observed| assert_eq!(observed.borrow().image_decodes, 1));
    assert!(content.tilesets.is_empty());
    let collection = content.domains.collection::<ProbeDomain>().unwrap();
    assert_eq!(collection.iter().len(), 3);
    for (tag, instance) in [(X, 1), (Y, 2), (Z, 3)] {
        let value = collection.get(&AssetRef::Product(product(tag))).unwrap();
        assert_eq!(value.instance, instance);
        assert_eq!(value.payload[0], tag);
        let acquisition = collection
            .runtime_acquisitions()
            .iter()
            .find(|asset| asset.reference() == product(tag))
            .unwrap();
        assert!(std::ptr::eq(value, acquisition.product()));
    }
    let record = content.scene.candidate().document().entities()[0]
        .components()
        .get(&ComponentTypeId::new(COMPONENT))
        .unwrap();
    assert_eq!(
        record.value().field("future").unwrap(),
        &encode_asset_reference_value(&AssetRef::Product(product(X)))
    );
}

#[test]
fn newly_revealed_earlier_definition_precedes_pending_z_for_both_recipe_orders() {
    let fixture = Fixture::new();
    for reversed in [false, true] {
        let base = fixture.open(reversed, content_limits());
        let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
        let definition = |tag| {
            base.plan()
                .runtime_codecs()
                .definition(base.member(product(tag)).unwrap())
                .unwrap()
        };
        assert!(definition(Y) < definition(X));
        assert!(definition(X) < definition(Z));
        assert!(base.member_index(product(Y)).unwrap() < base.member_index(product(X)).unwrap());
        reset_observations();
        let content = base.prepare_scene_content(product(0)).unwrap();
        assert_prepared_once(&content);
        assert_eq!(charge(&base, RuntimeContentBudgetKind::WorkBytes), 0);
        let escaped = content
            .domains
            .collection::<ProbeDomain>()
            .unwrap()
            .runtime_acquisitions()
            .iter()
            .find(|asset| asset.reference() == product(Y))
            .unwrap()
            .clone();
        drop(content);
        assert_eq!(dropped(), [X, Z]);
        assert_eq!(
            charge(&base, RuntimeContentBudgetKind::RetainedBytes),
            baseline + escaped.retained_bytes()
        );
        assert_eq!(escaped.product().instance, 2);
        drop(escaped);
        assert_eq!(dropped(), [X, Y, Z]);
        assert_refunded(&base, baseline);
    }
}

#[test]
fn exact_graph_decoder_work_overlap_passes_and_one_short_refuses_before_y_decode() {
    let fixture = Fixture::new();
    let peak = discovery_peak(&fixture);
    assert!(peak.work > Y_WORK);
    for reversed in [false, true] {
        for short in [false, true] {
            let limit = peak.work - usize::from(short);
            let base = fixture.open(
                reversed,
                content_limits().with_work_bytes(ByteLimit::new(limit).unwrap()),
            );
            let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
            reset_observations();
            let result = base.prepare_scene_content(product(0));
            if short {
                assert!(matches!(result, Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::WorkBytes, required, limit: actual,
                }) if required == peak.work && actual == limit));
                assert_eq!(decoded(), [X]);
                assert_eq!(dropped(), [X]);
            } else {
                let content = result.unwrap();
                assert_prepared_once(&content);
                drop(content);
                assert_eq!(dropped(), [X, Y, Z]);
            }
            assert_refunded(&base, baseline);
            // The failed closure must leave enough capacity for a real subsequent acquisition.
            let y = base
                .acquire_asset::<ProbeDomain, YCodec>(product(Y))
                .unwrap();
            drop(y);
            assert_refunded(&base, baseline);
        }
    }
}

#[test]
fn exact_live_output_and_next_decoder_promise_pass_and_one_short_refunds_every_owner() {
    let fixture = Fixture::new();
    let peak = discovery_peak(&fixture);
    for reversed in [false, true] {
        for short in [false, true] {
            let limit = peak.retained - usize::from(short);
            let base = fixture.open(
                reversed,
                content_limits().with_retained_bytes(ByteLimit::new(limit).unwrap()),
            );
            let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
            reset_observations();
            let result = base.prepare_scene_content(product(0));
            if short {
                assert!(matches!(result, Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes, required, limit: actual,
                }) if required == peak.retained && actual == limit));
                assert_eq!(decoded(), [X]);
                assert_eq!(dropped(), [X]);
            } else {
                let content = result.unwrap();
                assert_prepared_once(&content);
                drop(content);
                assert_eq!(dropped(), [X, Y, Z]);
            }
            assert_refunded(&base, baseline);
            let y = base
                .acquire_asset::<ProbeDomain, YCodec>(product(Y))
                .unwrap();
            drop(y);
            assert_refunded(&base, baseline);
        }
    }
}

#[test]
fn decoder_refusal_after_x_and_y_allocation_refunds_and_allows_a_fresh_preparation() {
    let fixture = Fixture::new();
    for reversed in [false, true] {
        let base = fixture.open(reversed, content_limits());
        let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
        reset_observations();
        REFUSE_Y.with(|refuse| refuse.set(true));
        assert!(matches!(
            base.prepare_scene_content(product(0)),
            Err(RuntimeContentError::Asset(_))
        ));
        assert_eq!(decoded(), [X, Y]);
        assert_eq!(dropped(), [X, Y]);
        assert_refunded(&base, baseline);
        reset_observations();
        let content = base.prepare_scene_content(product(0)).unwrap();
        assert_prepared_once(&content);
        drop(content);
        assert_eq!(dropped(), [X, Y, Z]);
        assert_refunded(&base, baseline);
    }
}

fn assert_deep_once(base: &RuntimeContentBase) {
    let report = base.validate_deep().unwrap();
    assert_eq!(report.catalog_digest(), base.catalog_digest());
    assert_eq!(report.members(), 5);
    assert_eq!(report.products(), 5);
    assert_eq!(decoded(), [X, Y, Z]);
    assert_eq!(dropped(), [X, Y, Z]);
    OBSERVED.with(|observed| assert_eq!(observed.borrow().image_decodes, 1));
}

#[test]
fn deep_validation_drops_each_heavy_output_before_the_next_decoder_and_visits_once() {
    let fixture = Fixture::new();
    for reversed in [false, true] {
        let base = fixture.open(reversed, content_limits());
        let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
        reset_observations();
        REQUIRE_PREVIOUS_DROP.with(|require| require.set(true));
        assert_deep_once(&base);
        assert_refunded(&base, baseline);
        // The report and compact facts leave no hidden decoded owner between validations.
        reset_observations();
        REQUIRE_PREVIOUS_DROP.with(|require| require.set(true));
        assert_deep_once(&base);
        assert_refunded(&base, baseline);
    }
}

#[test]
fn deep_compact_summary_overlaps_decoder_promises_at_exact_and_one_short_budgets() {
    let fixture = Fixture::new();
    let peak = discovery_peak(&fixture);
    assert!(peak.deep_work > Y_WORK);
    assert!(peak.deep_work < peak.work);
    assert!(peak.deep_retained < peak.retained);
    for reversed in [false, true] {
        for kind in [
            RuntimeContentBudgetKind::WorkBytes,
            RuntimeContentBudgetKind::RetainedBytes,
        ] {
            let exact = match kind {
                RuntimeContentBudgetKind::WorkBytes => peak.deep_work,
                RuntimeContentBudgetKind::RetainedBytes => peak.deep_retained,
                _ => unreachable!(),
            };
            for short in [false, true] {
                let limit = exact - usize::from(short);
                let limits = match kind {
                    RuntimeContentBudgetKind::WorkBytes => {
                        content_limits().with_work_bytes(ByteLimit::new(limit).unwrap())
                    }
                    RuntimeContentBudgetKind::RetainedBytes => {
                        content_limits().with_retained_bytes(ByteLimit::new(limit).unwrap())
                    }
                    _ => unreachable!(),
                };
                let base = fixture.open(reversed, limits);
                let baseline = charge(&base, RuntimeContentBudgetKind::RetainedBytes);
                reset_observations();
                REQUIRE_PREVIOUS_DROP.with(|require| require.set(true));
                if short {
                    assert!(
                        matches!(base.validate_deep(), Err(RuntimeContentError::Budget {
                        kind: actual_kind, required, limit: actual_limit,
                    }) if actual_kind == kind && required == exact && actual_limit == limit)
                    );
                    assert_eq!(decoded(), [X]);
                    assert_eq!(dropped(), [X]);
                    OBSERVED.with(|observed| assert_eq!(observed.borrow().image_decodes, 1));
                } else {
                    assert_deep_once(&base);
                }
                assert_refunded(&base, baseline);
                // Retry real work on this same base. A retained cap one byte below Y's own
                // promise necessarily still refuses Y, so use X for that independent retry.
                // Work-only refusals leave enough capacity for Y without graph/summary scratch.
                REQUIRE_PREVIOUS_DROP.with(|require| require.set(false));
                match kind {
                    RuntimeContentBudgetKind::RetainedBytes => {
                        let x = base
                            .acquire_asset::<ProbeDomain, XCodec>(product(X))
                            .unwrap();
                        drop(x);
                    }
                    RuntimeContentBudgetKind::WorkBytes => {
                        let y = base
                            .acquire_asset::<ProbeDomain, YCodec>(product(Y))
                            .unwrap();
                        drop(y);
                    }
                    _ => unreachable!(),
                }
                assert_refunded(&base, baseline);
            }
        }
    }
}
