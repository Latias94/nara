use std::{
    convert::Infallible,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_asset::{
    AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath,
    AssetSourcePath, RuntimeAssetCodec, SourceAssetImporter, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_fs::ContentDigest;
use nara_image::{ImageColorSpace, ImageExtent, ImageFormat};
use nara_material::{
    AlphaMode2d, Material2dDeclaration, Material2dRecord, Material2dRuntimeCodec, Material2dSource,
    MaterialParameterKind, MaterialParameterLayout, MaterialParameterValue, MaterialUniformField,
};
use nara_project::{ProductCapability, RuntimeConfiguration, RuntimePreset};
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneLimits, SceneComponentRecord, SceneDocument, SceneEntityId,
    SceneEntityRecord, SourceDocumentKey, SourceDocumentKind,
};

use super::super::{PreparedResidency, ProjectSceneResidency, publish_processed_startup};
use super::*;
use crate::{
    ProductRecipe,
    project_host::{
        RuntimeConfigurationPlan, open_explicit_product_project_root, resolve_runtime_configuration,
    },
    runtime_content::{
        ProcessedSceneContent, RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId,
        RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
};

mod sharing;

const MATERIAL: &str = "84d00000-0000-4000-8000-000000000001";
const STARTUP: &str = "84d00000-0000-4000-8000-000000000002";
const EMPTY: &str = "84d00000-0000-4000-8000-000000000003";
const IMAGE: &str = "84d00000-0000-4000-8000-000000000004";
const ADDITION: &str = "84d00000-0000-4000-8000-000000000005";
const OTHER_MATERIAL: &str = "84d00000-0000-4000-8000-000000000006";
const BODY: &str = r#"
fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> {
    return sampled * tint * material_parameters.tint;
}
"#;

fn product(source: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(source).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn material() -> Material2dAsset {
    import_material(product(MATERIAL), &material_source(), BODY)
}

fn material_source() -> Material2dSource {
    let declaration = Material2dDeclaration::new(
        "residency.material",
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
        Vec::new(),
    )
    .unwrap();
    Material2dSource::new(
        declaration,
        vec![AssetPath::new("material.wgsl").unwrap()],
        BTreeMap::from([(
            "color.tint".to_owned(),
            MaterialParameterValue::Vec4([0.25, 0.5, 0.75, 1.0]),
        )]),
        BTreeMap::new(),
    )
    .unwrap()
}

fn import_material(
    reference: AssetProductRef,
    source: &Material2dSource,
    body: &str,
) -> Material2dAsset {
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            reference.source(),
            AssetPath::new("material.json").unwrap(),
            material_kind(),
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

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_material_codec::<Material2dRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "nara.test.material-residency",
            1,
        ))
        .unwrap()
}

fn scene_bytes(
    plan: &RuntimeConfigurationPlan,
    reference: AssetProductRef,
    material: &Material2dAsset,
    populated: bool,
) -> Vec<u8> {
    let mut document = SceneDocument::default();
    if populated {
        let record = Material2dRecord::new(
            AssetRef::Product(material.product()),
            material.generation().declaration(),
            material.default_parameters().clone(),
            BTreeMap::new(),
        )
        .unwrap();
        document = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            SceneEntityId::new("material").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("nara.material2d"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                record.to_component_value().unwrap(),
            ),
        )]);
    }
    let source = SourceDocumentKey::stable_asset(reference.source(), SourceDocumentKind::Scene);
    let compiled = crate::startup_scene::compile_test_scene(
        &document,
        plan.schema_validation().bindings(),
        source,
    );
    let mut database = nara_asset::ProjectAssetDatabase::default();
    database
        .insert(AssetRecord::new(
            material.product().source(),
            AssetSourcePath::new("material.json").unwrap(),
            material_kind(),
        ))
        .unwrap();
    RuntimeSceneCandidate::cook(
        &compiled,
        plan.schema_validation().bindings(),
        database.resolver(&nara_asset::AssetPathScope::ROOT),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .to_json_bytes()
    .unwrap()
}

struct Fixture(PathBuf);

impl Fixture {
    fn open() -> (Self, RuntimeContentBase) {
        Self::with_material(material())
    }

    fn with_material(material: Material2dAsset) -> (Self, RuntimeContentBase) {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara_material_residency_{}_{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        let fixture = Self(root);
        let other = import_material(product(OTHER_MATERIAL), &material_source(), BODY);
        let recipe = recipe();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let mut members = Vec::new();
        for (path, reference, selected, populated) in [
            ("startup.runtime.json", product(STARTUP), &material, true),
            ("empty.runtime.json", product(EMPTY), &material, false),
            ("addition.runtime.json", product(ADDITION), &other, true),
        ] {
            let bytes = scene_bytes(&plan, reference, selected, populated);
            fs::write(fixture.0.join(path), &bytes).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&bytes),
                    RuntimeContentMemberKind::Scene {
                        source: SourceDocumentKey::stable_asset(
                            reference.source(),
                            SourceDocumentKind::Scene,
                        ),
                    },
                    vec![reference],
                    if populated {
                        vec![RuntimeContentDependency::new(
                            selected.product(),
                            RuntimeContentDependencyRole::Required,
                        )]
                    } else {
                        Vec::new()
                    },
                )
                .unwrap(),
            );
        }
        for (path, selected) in [
            ("material.runtime.json", &material),
            ("other.runtime.json", &other),
        ] {
            let bytes = Material2dRuntimeCodec::encode_runtime(selected).unwrap();
            fs::write(fixture.0.join(path), &bytes).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&bytes),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(Material2dRuntimeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![selected.product()],
                    Vec::new(),
                )
                .unwrap(),
            );
        }
        let target = RuntimeContentTarget::new("test-host", "material-residency").unwrap();
        let bytes = RuntimeContentCatalog::new(
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
                scenes: vec![product(STARTUP), product(EMPTY), product(ADDITION)],
                addressable_roots: Vec::new(),
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(fixture.0.join(RUNTIME_CONTENT_CATALOG_FILE), &bytes).unwrap();
        let base = RuntimeContentBase::open(
            &open_explicit_product_project_root(&fixture.0).unwrap(),
            &RuntimeContentExpectation::new(ContentDigest::of_bytes(&bytes), target),
            recipe,
            RuntimeContentLimits::default(),
        )
        .unwrap();
        (fixture, base)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.0.canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        assert_eq!(root.parent(), Some(temporary.as_path()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_material_residency_")
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(AssetServer::new());
    world.insert_resource(Assets::<Material2dAsset>::default());
    world.insert_resource(AssetStates::default());
    world.insert_resource(AssetEvents::default());
    world
}

fn retained_bytes(base: &RuntimeContentBase) -> usize {
    let cap = RuntimeContentLimits::default().retained_bytes().get();
    match base.reserve_retained(cap) {
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            limit,
        }) => {
            assert_eq!(limit, cap);
            required - cap
        }
        _ => panic!("the base already retains its immutable catalogue"),
    }
}

#[test]
fn processed_binding_outlives_residency_and_releases_original_acquisition_and_metadata() {
    let (_fixture, base) = Fixture::open();
    let baseline = retained_bytes(&base);
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    assert!(content.images.is_empty());
    assert!(content.tilesets.is_empty());
    let collection = content.domains.collection::<Material2dDomain>().unwrap();
    let acquisition_bytes = collection.runtime_acquisitions()[0].retained_bytes();
    let original_generation = collection.iter().next().unwrap().generation().clone();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let residency = world.resource::<ProjectSceneResidency>();
    let metadata = Arc::downgrade(&residency.stock.materials.owned[0].identity._metadata);
    let handle = residency.stock.materials.owned[0].handle;
    let published = world
        .resource::<Assets<Material2dAsset>>()
        .get(handle)
        .unwrap();
    assert_eq!(published.generation(), &original_generation);
    let binding = published
        .bind_values(BTreeMap::new(), BTreeMap::new(), |_| None)
        .unwrap();
    drop(original_generation);
    drop(content);
    drop(world);
    assert!(metadata.upgrade().is_some());
    assert!(retained_bytes(&base) >= baseline + acquisition_bytes);
    drop(binding);
    assert!(metadata.upgrade().is_none());
    assert_eq!(retained_bytes(&base), baseline);
}

#[test]
fn unpublished_material_slots_refuse_replaced_value_or_state_stores_without_publication() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let mut world = world();
    let prepared =
        PreparedResidency::prepare(&world, ResidencyInput::Processed(&content), None).unwrap();
    let handle = prepared
        .bindings
        .staged_server()
        .get_product_handle::<Material2dAsset>(product(MATERIAL))
        .unwrap();
    assert!(prepared.stock.materials.validate(&world).is_ok());
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_none()
    );
    assert!(world.resource::<AssetStates>().state(handle.id()).is_none());
    assert_eq!(world.resource::<AssetEvents>().iter().count(), 0);
    let values = std::mem::take(&mut *world.resource_mut::<Assets<Material2dAsset>>());
    assert!(prepared.stock.materials.validate(&world).is_err());
    *world.resource_mut::<Assets<Material2dAsset>>() = values;
    assert!(prepared.stock.materials.validate(&world).is_ok());
    world.insert_resource(AssetStates::default());
    assert!(prepared.stock.materials.validate(&world).is_err());
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_none()
    );
    assert_eq!(world.resource::<AssetEvents>().iter().count(), 0);
}

#[test]
fn raw_material_mutation_revokes_retirement_receipt() {
    let (_fixture, base) = Fixture::open();
    let content = base.prepare_scene_content(product(STARTUP)).unwrap();
    let empty = base.prepare_scene_content(product(EMPTY)).unwrap();
    let mut world = world();
    publish_processed_startup(&mut world, &content).unwrap();
    let current = world.resource::<ProjectSceneResidency>();
    let handle = current.stock.materials.owned[0].handle;
    let prepared =
        PreparedResidency::prepare(&world, ResidencyInput::Processed(&empty), Some(current))
            .unwrap();
    assert!(prepared.stock.materials.validate(&world).is_ok());
    world
        .resource_mut::<Assets<Material2dAsset>>()
        .get_mut(handle)
        .unwrap();
    assert!(prepared.stock.materials.validate(&world).is_err());
    assert!(
        world
            .resource::<Assets<Material2dAsset>>()
            .get(handle)
            .is_some()
    );
}

#[test]
fn incumbent_image_binding_cannot_satisfy_a_missing_candidate_texture() {
    let mut server = AssetServer::new();
    let reference = product(IMAGE);
    let handle = server.reserve_product::<ImageAsset>(reference).unwrap();
    let mut values = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let image = ImageAsset::from_product(
        reference,
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        vec![1, 2, 3, 255],
    )
    .unwrap();
    values
        .prepare_mutation(handle, &states)
        .unwrap()
        .commit_loaded(image, &mut values, &mut states, &mut events, None, None)
        .unwrap();
    assert!(values.get(handle).is_some());
    assert!(validate_image(&ImagePublications::empty(), &server, handle, reference).is_err());
}
