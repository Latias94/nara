use std::{
    collections::BTreeMap,
    convert::Infallible,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::fs::ContentDigest;
use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_asset::{
    AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
    AssetProductRef, AssetRecord, AssetRef, AssetReferenceDemand, AssetRuntimeError,
    AssetRuntimeLimits, AssetSourceKind, AssetSourcePath, ImportedProductId, ProjectAssetDatabase,
    RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueShape, PersistentComponentSchemaOwnerId,
    encode_asset_reference_value,
};
use nara_render::image::{
    ImageAsset, ImageColorSpace, ImageDomain, ImageExtent, ImageFormat, ImageRuntimeCodec,
};
use nara_render::material::{
    AlphaMode2d, Material2dDeclaration, Material2dDomain, Material2dRecord, Material2dRuntimeCodec,
    Material2dSource, MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue,
    MaterialTextureDeclaration, MaterialUniformField,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

use super::*;
use crate::{
    ProductRecipe, SchemaContribution,
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentAssetInner,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
        budget::DecodeBudget,
    },
};

const MATERIAL: &str = "84e00000-0000-4000-8000-000000000001";
const STARTUP: &str = "84e00000-0000-4000-8000-000000000002";
const IMAGE: &str = "84e00000-0000-4000-8000-000000000003";
const MATERIAL_CODEC: RuntimeCodecDefinitionId =
    RuntimeCodecDefinitionId::new("nara.test.runtime-material-custody", 1);
const FIXTURE_PREFIX: &str = "nara-runtime-material-custody-";
const BODY: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.tint;
}
"#;
const TEXTURED_BODY: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    let palette = textureLoad(palette_texture, vec2<i32>(0, 0), 0);
    return sampled * tint * material_parameters.tint * palette;
}
"#;

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn material(textured: bool) -> Material2dAsset {
    let (slots, textures, body) = if textured {
        (
            vec![MaterialTextureDeclaration::new("palette", "palette_texture", 1).unwrap()],
            BTreeMap::from([("palette".to_owned(), AssetRef::Product(product(IMAGE)))]),
            TEXTURED_BODY,
        )
    } else {
        (Vec::new(), BTreeMap::new(), BODY)
    };
    let declaration = Material2dDeclaration::new(
        "runtime.custody.material",
        1,
        AlphaMode2d::Blend,
        MaterialParameterLayout::new(
            vec![
                MaterialUniformField::new("color.tint", "tint", MaterialParameterKind::Vec4, 0)
                    .unwrap(),
            ],
            16,
        )
        .unwrap(),
        slots,
    )
    .unwrap();
    let source = Material2dSource::new(
        declaration,
        vec![AssetPath::new("material.wgsl").unwrap()],
        BTreeMap::from([(
            "color.tint".to_owned(),
            MaterialParameterValue::Vec4([0.25, 0.5, 0.75, 1.0]),
        )]),
        textures,
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            product(MATERIAL).source(),
            AssetPath::new("material.json").unwrap(),
            AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        ),
        (),
    );
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes_owned(
        metadata
            .to_json_string()
            .unwrap()
            .into_bytes()
            .into_boxed_slice(),
    )
    .unwrap();
    let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, Infallible> {
        Ok(Arc::from(body.as_bytes()))
    };
    Material2dDomain::prepare(&mut AssetImportSource::new(
        &metadata,
        Arc::from(source.encode().unwrap()),
        &mut reader,
    ))
    .unwrap()
}

struct Fixture {
    root: PathBuf,
    recipe: ProductRecipe,
    expected: RuntimeContentExpectation,
    material_bytes: Box<[u8]>,
}

impl Fixture {
    fn new<C: RuntimeAssetCodec<Material2dAsset>>() -> Self {
        let recipe = ProductRecipe::new()
            .add_runtime_material_codec::<C>(MATERIAL_CODEC)
            .unwrap();
        Self::write::<C>(recipe, true, None)
    }

    fn with_default_texture<C: RuntimeAssetCodec<Material2dAsset>>(
        native: bool,
        demand: RuntimeContentDependencyRole,
    ) -> Self {
        let image_codec = RuntimeCodecDefinitionId::new("nara.test.runtime-material-image", 1);
        let recipe = if native {
            ProductRecipe::new()
                .add_runtime_material_codec::<C>(MATERIAL_CODEC)
                .unwrap()
                .add_runtime_image_codec::<ImageRuntimeCodec>(image_codec)
                .unwrap()
        } else {
            ProductRecipe::new()
                .add_contribution(cpu_material_contribution())
                .unwrap()
                .add_runtime_asset_codec::<Material2dDomain, C>(MATERIAL_CODEC)
                .unwrap()
                .add_runtime_asset_codec::<ImageDomain, ImageRuntimeCodec>(image_codec)
                .unwrap()
        };
        Self::write::<C>(recipe, native, Some(demand))
    }

    fn write<C: RuntimeAssetCodec<Material2dAsset>>(
        recipe: ProductRecipe,
        native_carrier: bool,
        texture: Option<RuntimeContentDependencyRole>,
    ) -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_PREFIX}{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let material = material(texture.is_some());
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let reference = AssetRef::Product(material.product());
        let (component, record) = if native_carrier {
            let record = Material2dRecord::new(
                reference,
                material.generation().declaration(),
                material.default_parameters().clone(),
                BTreeMap::new(),
            )
            .unwrap();
            (
                ComponentTypeId::new("nara.material2d"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    record.to_component_value().unwrap(),
                ),
            )
        } else {
            (
                ComponentTypeId::new(CPU_MATERIAL_COMPONENT),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    encode_asset_reference_value(&reference),
                )
                .with_owner(PersistentComponentSchemaOwnerId::try_new(CPU_MATERIAL_OWNER).unwrap()),
            )
        };
        let document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("material").unwrap(),
        )
        .with_component(component, record)]);
        let source =
            SourceDocumentKey::stable_asset(product(STARTUP).source(), SourceDocumentKind::Scene);
        let compiled = crate::startup_scene::compile_test_scene(
            &document,
            plan.schema_validation().bindings(),
            source.clone(),
        );
        let mut database = ProjectAssetDatabase::default();
        database
            .insert(AssetRecord::new(
                material.product().source(),
                AssetSourcePath::new("material.json").unwrap(),
                AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
            ))
            .unwrap();
        let scene_bytes = RuntimeSceneCandidate::cook(
            &compiled,
            plan.schema_validation().bindings(),
            database.resolver(&nara_asset::AssetPathScope::ROOT),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(root.join("startup.runtime.json"), &scene_bytes).unwrap();
        let material_bytes = C::encode_runtime(&material).unwrap();
        fs::write(root.join("material.runtime.json"), &material_bytes).unwrap();
        let mut members = vec![
            RuntimeContentMember::new(
                "startup.runtime.json",
                ContentDigest::of_bytes(&scene_bytes),
                RuntimeContentMemberKind::Scene { source },
                vec![product(STARTUP)],
                vec![RuntimeContentDependency::new(
                    material.product(),
                    RuntimeContentDependencyRole::Required,
                )],
            )
            .unwrap(),
            RuntimeContentMember::new(
                "material.runtime.json",
                ContentDigest::of_bytes(&material_bytes),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(C::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::ONE,
                },
                vec![material.product()],
                texture
                    .map(|role| RuntimeContentDependency::new(product(IMAGE), role))
                    .into_iter()
                    .collect(),
            )
            .unwrap(),
        ];
        if texture.is_some() {
            let image = ImageAsset::from_product(
                product(IMAGE),
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Srgb,
                vec![24, 120, 220, 255],
            )
            .unwrap();
            let bytes = ImageRuntimeCodec::encode_runtime(&image).unwrap();
            fs::write(root.join("image.runtime.bin"), &bytes).unwrap();
            members.push(
                RuntimeContentMember::new(
                    "image.runtime.bin",
                    ContentDigest::of_bytes(&bytes),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::new(ImageRuntimeCodec::RUNTIME_VERSION).unwrap(),
                    },
                    vec![product(IMAGE)],
                    Vec::new(),
                )
                .unwrap(),
            );
        }
        let target = RuntimeContentTarget::new("test-host", "runtime-material-custody").unwrap();
        let catalog_bytes = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target.clone(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
                startup_scene: product(STARTUP),
                scenes: vec![product(STARTUP)],
                addressable_roots: Vec::new(),
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(root.join(RUNTIME_CONTENT_CATALOG_FILE), &catalog_bytes).unwrap();
        Self {
            root,
            recipe,
            expected: RuntimeContentExpectation::new(
                ContentDigest::of_bytes(&catalog_bytes),
                target,
            ),
            material_bytes,
        }
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
}

const CPU_MATERIAL_OWNER: &str = "nara.test.cpu-material";
const CPU_MATERIAL_COMPONENT: &str = "nara.test.CpuMaterial";

#[derive(Default)]
struct CpuMaterialConsumer;

impl Plugin for CpuMaterialConsumer {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new(CPU_MATERIAL_OWNER), PluginCategory::Runtime);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn cpu_material_contribution() -> SchemaContribution<CpuMaterialConsumer> {
    let schema = ComponentSchema::new(
        ComponentTypeId::new(CPU_MATERIAL_COMPONENT),
        "CPU Material",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("material"),
        "Material",
        ComponentFieldPath::empty(),
        ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new(CPU_MATERIAL_OWNER).unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    SchemaContribution::data_for_default([owner]).unwrap()
}

#[test]
fn native_material_defaults_require_their_own_immediate_image_edge() {
    let fixture = Fixture::with_default_texture::<DeferredTextureCodec>(
        true,
        RuntimeContentDependencyRole::Reference,
    );
    let base = fixture.open(RuntimeContentLimits::default());
    for (operation, result) in [
        ("deep validation", base.validate_deep().map(|_| ())),
        (
            "Scene preparation",
            base.prepare_scene_content(product(STARTUP)).map(|_| ()),
        ),
    ] {
        let error = result.expect_err("a native Material default requires its Image immediately");
        assert!(
            matches!(
                error,
                RuntimeContentError::DependencyMismatch {
                    product: target,
                    declared: Some(RuntimeContentDependencyRole::Reference),
                    actual: Some(AssetReferenceDemand::Immediate),
                } if target == product(IMAGE)
            ),
            "{operation} returned {error:?}"
        );
    }

    let fixture = Fixture::with_default_texture::<Material2dRuntimeCodec>(
        true,
        RuntimeContentDependencyRole::Required,
    );
    let base = fixture.open(RuntimeContentLimits::default());
    assert_eq!(base.validate_deep().unwrap().members(), 3);
    let prepared = base.prepare_scene_content(product(STARTUP)).unwrap();
    assert_eq!(prepared.materials.len(), 1);
    assert_eq!(prepared.images.len(), 1);
    assert_eq!(prepared.images[0].reference(), product(IMAGE));
}

#[test]
fn cpu_material_defaults_can_defer_their_typed_image() {
    let fixture = Fixture::with_default_texture::<DeferredTextureCodec>(
        false,
        RuntimeContentDependencyRole::Reference,
    );
    let base = fixture.open(RuntimeContentLimits::default());
    assert_eq!(base.validate_deep().unwrap().members(), 3);
    let prepared = base.prepare_scene_content(product(STARTUP)).unwrap();
    assert!(prepared.materials.is_empty());
    assert!(prepared.images.is_empty());
    assert!(prepared.domains.collection::<ImageDomain>().is_none());
    let material = prepared
        .domains
        .collection::<Material2dDomain>()
        .unwrap()
        .get(&AssetRef::Product(product(MATERIAL)))
        .unwrap();
    let texture = material.textures().get("palette").unwrap();
    assert_eq!(texture, &AssetRef::Product(product(IMAGE)));
    let image = base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(product(IMAGE))
        .unwrap();
    assert_eq!(image.product().pixels(), [24, 120, 220, 255]);
}

struct DeferredTextureCodec;

impl RuntimeAssetCodec<Material2dAsset> for DeferredTextureCodec {
    const RUNTIME_KIND: &'static str = Material2dRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = Material2dRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        Material2dRuntimeCodec::runtime_limits()
    }

    fn encode_runtime(product: &Material2dAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        Material2dRuntimeCodec::encode_runtime(product)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<Material2dAsset, AssetRuntimeError> {
        Material2dRuntimeCodec::decode_runtime(bytes)
    }

    fn runtime_retained_bytes(product: &Material2dAsset) -> Result<usize, AssetRuntimeError> {
        Material2dRuntimeCodec::runtime_retained_bytes(product)
    }

    fn visit_runtime_dependencies<E>(
        product: &Material2dAsset,
        mut visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Material2dRuntimeCodec::visit_runtime_dependencies(product, |target, _| {
            visitor(target, AssetReferenceDemand::Deferred)
        })
    }

    fn contains_runtime_product(product: &Material2dAsset, reference: AssetProductRef) -> bool {
        Material2dRuntimeCodec::contains_runtime_product(product, reference)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(root.parent(), Some(temporary.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(FIXTURE_PREFIX)
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn charge(budget: &Arc<DecodeBudget>, limits: RuntimeContentLimits, work: bool) -> usize {
    let (kind, retained, working, probe) = if work {
        let limit = limits.work_bytes().get();
        (RuntimeContentBudgetKind::WorkBytes, 0, limit, limit)
    } else {
        let limit = limits.retained_bytes().get();
        (RuntimeContentBudgetKind::RetainedBytes, limit, 0, limit)
    };
    match budget.reserve(retained, working) {
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

#[test]
fn scene_binding_reserves_known_component_cost_before_parsing() {
    let fixture = Fixture::new::<Material2dRuntimeCodec>();
    let limits = RuntimeContentLimits::default();
    let base = fixture.open(limits);
    let scene = base.acquire_startup_scene().unwrap();
    let component = scene.candidate().document().entities()[0]
        .components()
        .get(&ComponentTypeId::new("nara.material2d"))
        .unwrap();
    let budget = &base.inner.budget;
    let limit = limits.work_bytes().get();
    let scratch = material_record_work_bytes(component.value_cost()).unwrap();
    let in_use = budget.reserve(0, limit).unwrap();
    assert!(matches!(
        SceneMaterialBinding::read(&base, component),
        Err(RuntimeContentError::Budget { kind: RuntimeContentBudgetKind::WorkBytes, required, limit: actual })
            if required == limit + scratch && actual == limit
    ));
    drop(in_use);

    let binding = SceneMaterialBinding::read(&base, component).unwrap();
    assert_eq!(
        binding.record.material(),
        &AssetRef::Product(product(MATERIAL))
    );
    assert_eq!(charge(budget, limits, true), scratch);
    drop(binding);
    assert_eq!(charge(budget, limits, true), 0);
}

fn assert_snapshot_custody<S>(
    capture: impl FnOnce(&Material2dAsset) -> S,
    inspect: impl FnOnce(&S),
) {
    let fixture = Fixture::new::<Material2dRuntimeCodec>();
    let limits = RuntimeContentLimits::default();
    let base = fixture.open(limits);
    let budget = Arc::clone(&base.inner.budget);
    let origin = Arc::downgrade(&base.inner);
    let baseline = charge(&budget, limits, false);
    let acquisition = base
        .acquire_asset::<Material2dDomain, Material2dRuntimeCodec>(product(MATERIAL))
        .unwrap();
    let wrapper = Arc::downgrade(&acquisition.inner);
    let retained = acquisition.retained_bytes();
    assert!(acquisition.inner._lease.is_none());
    assert!(acquisition.product().generation().inputs().is_empty());
    assert_eq!(charge(&budget, limits, false), baseline + retained);
    assert_eq!(charge(&budget, limits, true), 0);
    let snapshot = capture(acquisition.product());
    drop(acquisition);
    drop(base);
    drop(fixture);
    assert!(wrapper.upgrade().is_none());
    assert!(origin.upgrade().is_some());
    assert_eq!(charge(&budget, limits, false), baseline + retained);
    inspect(&snapshot);
    drop(snapshot);
    assert!(origin.upgrade().is_none());
    assert_eq!(charge(&budget, limits, false), baseline);
    assert_eq!(charge(&budget, limits, true), 0);
}

#[test]
fn public_shader_snapshot_keeps_original_runtime_custody() {
    assert_snapshot_custody(
        |asset| asset.generation().shader().clone(),
        |shader| assert_eq!(shader.user_source(), format!("{BODY}\n")),
    );
}

#[test]
fn public_generation_snapshot_keeps_original_runtime_custody() {
    assert_snapshot_custody(
        |asset| asset.generation().clone(),
        |generation| assert_eq!(generation.shader().user_source(), format!("{BODY}\n")),
    );
}

#[test]
fn public_product_clone_keeps_original_runtime_custody() {
    assert_snapshot_custody(Clone::clone, |asset| {
        assert_eq!(asset.product(), product(MATERIAL));
        assert_eq!(
            asset.generation().shader().user_source(),
            format!("{BODY}\n")
        );
        assert!(Material2dRuntimeCodec::runtime_retained_bytes(asset).is_err());
    });
}

#[test]
fn processed_collection_product_and_generation_share_one_original_runtime_charge() {
    let fixture = Fixture::new::<Material2dRuntimeCodec>();
    let limits = RuntimeContentLimits::default();
    let base = fixture.open(limits);
    let budget = Arc::clone(&base.inner.budget);
    let origin = Arc::downgrade(&base.inner);
    let baseline = charge(&budget, limits, false);
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let collection = content.domains.collection::<Material2dDomain>().unwrap();
    assert_eq!(collection.runtime_acquisitions().len(), 1);
    let acquisition = &collection.runtime_acquisitions()[0];
    let wrapper = Arc::downgrade(&acquisition.inner);
    let retained = acquisition.retained_bytes();
    let asset = collection.iter().next().unwrap().clone();
    let generation = collection
        .get(&AssetRef::Product(product(MATERIAL)))
        .unwrap()
        .generation()
        .clone();
    assert_eq!(asset.generation(), &generation);
    drop(content);
    drop(base);
    drop(fixture);
    assert!(wrapper.upgrade().is_none());
    assert!(origin.upgrade().is_some());
    assert_eq!(charge(&budget, limits, false), baseline + retained);
    assert_eq!(charge(&budget, limits, true), 0);
    assert_eq!(
        asset.generation().shader().user_source(),
        format!("{BODY}\n")
    );
    drop(asset);
    assert_eq!(charge(&budget, limits, false), baseline + retained);
    assert_eq!(generation.shader().user_source(), format!("{BODY}\n"));
    drop(generation);
    assert!(origin.upgrade().is_none());
    assert_eq!(charge(&budget, limits, false), baseline);
}

const CODEC_RETAINED: usize = 32 * 1024;

struct FixedMaterialCodec;

impl RuntimeAssetCodec<Material2dAsset> for FixedMaterialCodec {
    const RUNTIME_KIND: &'static str = Material2dRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = Material2dRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        let limits = Material2dRuntimeCodec::runtime_limits();
        AssetRuntimeLimits::new(
            limits.encoded_bytes(),
            limits.work_bytes(),
            ByteLimit::new(CODEC_RETAINED).unwrap(),
        )
    }

    fn encode_runtime(product: &Material2dAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        Material2dRuntimeCodec::encode_runtime(product)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<Material2dAsset, AssetRuntimeError> {
        Material2dRuntimeCodec::decode_runtime(bytes)
    }

    fn runtime_retained_bytes(product: &Material2dAsset) -> Result<usize, AssetRuntimeError> {
        let actual = Material2dRuntimeCodec::runtime_retained_bytes(product)?;
        assert!(actual <= CODEC_RETAINED);
        Ok(CODEC_RETAINED)
    }

    fn visit_runtime_dependencies<E>(
        product: &Material2dAsset,
        visitor: impl FnMut(AssetProductRef, AssetReferenceDemand) -> Result<(), E>,
    ) -> Result<(), E> {
        Material2dRuntimeCodec::visit_runtime_dependencies(product, visitor)
    }

    fn contains_runtime_product(product: &Material2dAsset, reference: AssetProductRef) -> bool {
        Material2dRuntimeCodec::contains_runtime_product(product, reference)
    }
}

#[test]
fn host_metadata_is_pre_admitted_independently_of_the_original_codec_ceiling() {
    let fixture = Fixture::new::<FixedMaterialCodec>();
    let raw = FixedMaterialCodec::decode_runtime(&fixture.material_bytes).unwrap();
    let original = FixedMaterialCodec::runtime_retained_bytes(&raw).unwrap()
        + size_of::<RuntimeContentAssetInner<Material2dAsset>>()
        - size_of::<Material2dAsset>()
        + crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;
    let extra = raw.generation().lifetime_owner_retained_bytes().unwrap()
        + size_of::<RuntimeMaterialOwner>()
        + crate::scene_retention::ARC_CONTROL_BLOCK_BYTES;
    let base = fixture.open(RuntimeContentLimits::default());
    let baseline = charge(&base.inner.budget, RuntimeContentLimits::default(), false);
    drop(base);
    assert!(extra > 0);
    assert_eq!(
        FixedMaterialCodec::runtime_limits().retained_bytes().get(),
        CODEC_RETAINED,
    );
    for enough in [false, true] {
        let limit = baseline + original + extra - usize::from(!enough);
        let limits =
            RuntimeContentLimits::default().with_retained_bytes(ByteLimit::new(limit).unwrap());
        let base = fixture.open(limits);
        let acquired =
            base.acquire_asset::<Material2dDomain, FixedMaterialCodec>(product(MATERIAL));
        if enough {
            let acquired = acquired.unwrap();
            assert_eq!(acquired.retained_bytes(), original + extra);
            assert_eq!(charge(&base.inner.budget, limits, false), limit);
            assert_eq!(
                acquired.product().generation().shader(),
                raw.generation().shader(),
            );
            assert_eq!(
                acquired.product().default_parameters(),
                raw.default_parameters(),
            );
            assert_eq!(acquired.product().textures(), raw.textures());
            assert!(FixedMaterialCodec::runtime_retained_bytes(acquired.product()).is_err());
            drop(acquired);
        } else {
            assert!(matches!(
                acquired,
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                    required,
                    limit: actual_limit,
                }) if required == limit + 1 && actual_limit == limit
            ));
        }
        assert_eq!(charge(&base.inner.budget, limits, false), baseline);
        assert_eq!(charge(&base.inner.budget, limits, true), 0);
    }
}
