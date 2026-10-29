//! Explicit Runtime Image ownership through the public content, Scene and render boundaries.
//!
//! This consumer owns native residency. It does not install a generic packaged Player or infer
//! GPU upload, publication authority or source provenance from a successful CPU decode.

use std::{
    cell::{Cell, RefCell},
    fs,
    mem::size_of,
    sync::{Arc, mpsc},
    thread,
    time::Duration,
};

use nara_asset::{
    AssetDomain, AssetEvents, AssetPath, AssetProductRef, AssetRecord, AssetRef, AssetRuntimeError,
    AssetRuntimeLimits, AssetServer, AssetSourceKind, AssetStates, Assets, Handle,
    ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion, ItemLimit};
use nara_ecs::{Component, Entity, Resource, World};
use nara_engine::{
    ProductRecipe, SchemaContribution,
    app::{
        App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    fs::ContentDigest,
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{open_explicit_product_project_root, resolve_runtime_configuration},
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeContentAsset, RuntimeContentBase,
        RuntimeContentBudgetKind, RuntimeContentCatalog, RuntimeContentCatalogDefinition,
        RuntimeContentCatalogLimits, RuntimeContentDependency, RuntimeContentDependencyRole,
        RuntimeContentError, RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentMember,
        RuntimeContentMemberKind, RuntimeContentTarget,
    },
};
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageContentHash, ImageContentSnapshot,
    ImageDomain, ImageExtent, ImageFormat, ImageImportSettings, ImageImporter, ImageRuntimeCodec,
    PreparedImageSnapshot,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
    ComponentFieldSchema, ComponentRegistry, ComponentRegistryError, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition, ComponentSchemaProviderSourceError, ComponentSchemaVersion,
    ComponentTypeId, ComponentValueShape, PreparedComponentCandidate, decode_asset_reference_value,
    encode_asset_reference_value,
};
use nara_render::RenderResourceContentHash;
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits,
    SceneComponentBindings, SceneEntityId, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind,
    advanced::{
        SceneAssetBindings, SceneNativeRetirementClaims, SceneProductResource,
        SceneProductTransactionLimits,
    },
    retire_and_despawn_scene_instance,
};

const IMAGE_SOURCE: &str = "c3853676-314f-436d-844d-10dd534bbcca";
const AUXILIARY_ID: &str = "a93115ac-5128-4280-ab56-7d458a3da7b8";
const ABSENT_ID: &str = "5f1e06f7-2a25-42b0-b384-31c202fe931c";
const FOREIGN_SOURCE: &str = "2e5346d1-3ee3-4552-a232-dee8784781f8";
const STARTUP_SOURCE: &str = "bf2f54a7-911d-433a-83f4-9742512c320c";
const LATER_SOURCE: &str = "9396e58d-5f96-4e4f-a29d-a1987e017872";
const PORTRAIT: &str = "consumer.runtime-image.Portrait";
const SOURCE_FILE: &str = "authoring.png";
const PRIMARY_FILE: &str = "primary.runtime.image";
const AUXILIARY_FILE: &str = "auxiliary.runtime.image";
const STARTUP_FILE: &str = "startup.runtime.json";
const LATER_FILE: &str = "later.runtime.json";
const SOURCE_PNG: &[u8] = include_bytes!("../../../../reference-game/assets/textures/player.png");
const AUXILIARY_PIXELS: &[u8] = &[19, 37, 53, 255, 71, 89, 101, 127];

#[derive(Component)]
struct Portrait(Handle<ImageAsset>);

/// The application owns the actual values and their matching publication-state store together.
#[derive(Resource, Default)]
struct ImageProducts {
    images: Assets<ImageAsset>,
    states: AssetStates,
    events: AssetEvents,
    // Every publication rotates this owner identity, including equal-content writes.
    revision: Arc<()>,
}

impl SceneProductResource for ImageProducts {
    type Baseline = Arc<()>;

    fn capture_baseline(&self) -> Self::Baseline {
        Arc::clone(&self.revision)
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        Arc::ptr_eq(&self.revision, baseline)
    }
}

impl ImageProducts {
    fn publish(&mut self, handle: Handle<ImageAsset>, value: ImageAsset) {
        self.images
            .commit_loaded(
                handle,
                value,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .unwrap();
        self.revision = Arc::new(());
        let state = self.states.state(handle.id()).unwrap();
        assert!(state.usable_version().is_some());
        assert!(state.source_hash().is_none());
        assert!(state.import_hash().is_none());
    }

    fn capture(&self, world: &World, handle: Handle<ImageAsset>) -> ImageContentSnapshot {
        ImageContentSnapshot::capture(
            handle,
            world.resource::<AssetServer>(),
            &self.states,
            &self.images,
        )
        .unwrap()
    }
}

fn schema() -> ComponentSchema {
    ComponentSchema::new(
        ComponentTypeId::new(PORTRAIT),
        "Portrait",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([ComponentFieldSchema::required(
        ComponentFieldId::new("image"),
        "Image",
        ComponentFieldPath::empty(),
        ComponentValueShape::AssetRef,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef)])
}

fn catalog() -> Result<ComponentSchemaCatalog, ComponentSchemaProviderSourceError> {
    Ok(ComponentSchemaCatalog {
        components: vec![schema()],
        ..Default::default()
    })
}

fn register(registry: &mut ComponentRegistry) -> Result<(), ComponentRegistryError> {
    registry.register_persistent_component_codec_with_context::<Portrait, _, _>(
        schema(),
        |value, _| {
            let reference = decode_asset_reference_value(value)?;
            Ok(PreparedComponentCandidate::with_asset_server(
                move |context| {
                    context
                        .resolve_asset_ref::<ImageAsset>(&reference)
                        .map(Portrait)
                        .map_err(|error| ComponentCodecError::Message(error.to_string()))
                },
            ))
        },
        |portrait, world, context| {
            let reference = AssetRef::from_handle_with_policy(
                world.resource::<AssetServer>(),
                portrait.0,
                context.asset_ref_export_policy(),
            )
            .map_err(|error| ComponentCodecError::Message(error.to_string()))?;
            Ok(encode_asset_reference_value(&reference))
        },
    )?;
    Ok(())
}

const PROVIDER_ID: PluginSchemaProviderId =
    PluginSchemaProviderId::new("consumer.runtime-image.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.runtime-image"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.runtime-image.native", 1),
    catalog,
    register,
);

#[derive(Default)]
struct ImageConsumerPlugin;

impl Plugin for ImageConsumerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.runtime-image"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID])
        .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, _: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(
            SchemaContribution::<ImageConsumerPlugin>::for_default([PROVIDER]).unwrap(),
        )
        .unwrap()
}

fn stable(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn product(source: &str, selection: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(stable(source), selection).unwrap()
}

fn primary() -> AssetProductRef {
    product(IMAGE_SOURCE, ImportedProductId::PRIMARY)
}

fn auxiliary() -> AssetProductRef {
    product(
        IMAGE_SOURCE,
        ImportedProductId::parse_subproduct(AUXILIARY_ID).unwrap(),
    )
}

fn absent() -> AssetProductRef {
    product(
        IMAGE_SOURCE,
        ImportedProductId::parse_subproduct(ABSENT_ID).unwrap(),
    )
}

fn startup() -> AssetProductRef {
    product(STARTUP_SOURCE, ImportedProductId::PRIMARY)
}

fn later() -> AssetProductRef {
    product(LATER_SOURCE, ImportedProductId::PRIMARY)
}

fn source_key(source: &str) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(stable(source), SourceDocumentKind::Scene)
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("consumer.runtime-image", "current").unwrap()
}

fn portrait_id() -> SceneEntityId {
    SceneEntityId::new("portrait").unwrap()
}

fn target() -> RuntimeContentTarget {
    RuntimeContentTarget::new("x86_64-pc-windows-msvc", "runtime-image-consumer").unwrap()
}

fn bytes(value: usize) -> ByteLimit {
    ByteLimit::new(value).unwrap()
}

/// A fixed producer fixture for the public processed format; no source database is invented.
fn processed_scene(
    bindings: &SceneComponentBindings,
    source: &str,
    reference: AssetProductRef,
) -> RuntimeSceneCandidate {
    let selection = match reference.product().as_subproduct() {
        None => r#"{"type":"map","value":{"kind":{"type":"string","value":"primary"}}}"#.to_owned(),
        Some(id) => format!(
            r#"{{"type":"map","value":{{"kind":{{"type":"string","value":"subproduct"}},"id":{{"type":"string","value":"{id}"}}}}}}"#,
        ),
    };
    let encoded = r#"{
        "kind":"runtime_scene","format_version":1,"engine_min_version":"0.1.0",
        "generator":{"name":"runtime-image-consumer","version":"0.0.0"},
        "payload":{
            "source_key":{"kind":"scene","origin":{"stable_asset":{"stable_id":"@SCENE@"}}},
            "roots":["portrait"],
            "entities":[{"id":"portrait","children":[],"components":{
                "consumer.runtime-image.Portrait":{"version":1,"value":{
                    "type":"map","value":{
                        "kind":{"type":"string","value":"product"},
                        "value":{"type":"map","value":{
                            "source":{"type":"string","value":"@SOURCE@"},
                            "product":@SELECTION@
                        }}
                    }
                }}
            }}],
            "provenance":[{"entity":"portrait","local_entity":"portrait","origin":{"kind":"local"}}]
        }
    }"#
    .replace("@SCENE@", source)
    .replace("@SOURCE@", &reference.source().to_string())
    .replace("@SELECTION@", &selection);
    RuntimeSceneDocumentCandidate::from_json_bytes(
        encoded.as_bytes(),
        RuntimeSceneLimits::default(),
    )
    .unwrap()
    .admit(&source_key(source), bindings)
    .unwrap()
}

struct Fixture {
    directory: tempfile::TempDir,
    catalog: RuntimeContentCatalog,
    expected: RuntimeContentExpectation,
    primary_pixels: Vec<u8>,
    primary_extent: ImageExtent,
    primary_hash: ImageContentHash,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(SOURCE_FILE), SOURCE_PNG).unwrap();
        let imported = ImageImporter::new()
            .unwrap()
            .preflight_unpublished_import(ImageBytesImportRequest::new(
                AssetRecord::new(
                    stable(IMAGE_SOURCE),
                    AssetPath::new(SOURCE_FILE).unwrap(),
                    AssetSourceKind::Image,
                ),
                fs::read(directory.path().join(SOURCE_FILE)).unwrap(),
                ImageImportSettings::new(ImageColorSpace::Srgb),
            ))
            .unwrap()
            .import()
            .unwrap();
        assert!(imported.source().is_some());
        assert_eq!(imported.product(), Some(primary()));
        let primary_pixels = imported.pixels().to_vec();
        let primary_extent = imported.extent();
        let primary_hash = imported.content_hash();
        let primary_bytes = ImageRuntimeCodec::encode_runtime(&imported).unwrap();
        let metadata_length = u32::from_le_bytes(primary_bytes[4..8].try_into().unwrap()) as usize;
        let metadata = std::str::from_utf8(&primary_bytes[8..8 + metadata_length]).unwrap();
        for forbidden in [
            SOURCE_FILE,
            "\"settings\"",
            "\"artifact\"",
            "\"source_hash\"",
            "\"content_hash\"",
        ] {
            assert!(
                !metadata.contains(forbidden),
                "Runtime metadata contains {forbidden}"
            );
        }
        drop(imported);
        let auxiliary_image = ImageAsset::from_product(
            auxiliary(),
            ImageExtent::new(2, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            AUXILIARY_PIXELS.to_vec(),
        )
        .unwrap();
        let auxiliary_bytes = ImageRuntimeCodec::encode_runtime(&auxiliary_image).unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe()).unwrap();
        let mut members = Vec::new();
        for (path, source, scene_product, reference) in [
            (STARTUP_FILE, STARTUP_SOURCE, startup(), primary()),
            (LATER_FILE, LATER_SOURCE, later(), auxiliary()),
        ] {
            let encoded = processed_scene(plan.schema_validation().bindings(), source, reference)
                .to_json_bytes()
                .unwrap();
            fs::write(directory.path().join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: source_key(source),
                    },
                    vec![scene_product],
                    vec![RuntimeContentDependency::new(
                        reference,
                        RuntimeContentDependencyRole::Required,
                    )],
                )
                .unwrap(),
            );
        }
        for (path, reference, encoded) in [
            (PRIMARY_FILE, primary(), primary_bytes),
            (AUXILIARY_FILE, auxiliary(), auxiliary_bytes),
        ] {
            fs::write(directory.path().join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![reference],
                    vec![],
                )
                .unwrap(),
            );
        }
        let catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: target(),
                configuration,
                plugin_plan_fingerprint: plan.plugin_plan().fingerprint().as_bytes(),
                schema_fingerprint: plan
                    .schema_validation()
                    .composition_fingerprint()
                    .as_bytes(),
                startup_scene: startup(),
                scenes: vec![startup(), later()],
                addressable_roots: vec![primary(), auxiliary()],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(
            directory.path().join(RUNTIME_CONTENT_CATALOG_FILE),
            &encoded,
        )
        .unwrap();
        Self {
            directory,
            catalog,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target()),
            primary_pixels,
            primary_extent,
            primary_hash,
        }
    }

    fn small_images() -> Self {
        Self::with_primary_image(
            ImageAsset::from_product(
                primary(),
                ImageExtent::new(2, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Srgb,
                vec![3, 5, 7, 255, 11, 13, 17, 127],
            )
            .unwrap(),
        )
    }

    fn with_primary_image(image: ImageAsset) -> Self {
        let mut fixture = Self::new();
        let encoded = ImageRuntimeCodec::encode_runtime(&image).unwrap();
        fixture.replace_primary(&encoded, ImageRuntimeCodec::RUNTIME_KIND);
        fixture.primary_pixels = image.pixels().to_vec();
        fixture.primary_extent = image.extent();
        fixture.primary_hash = image.content_hash();
        fixture
    }

    fn open(
        &self,
        limits: RuntimeContentLimits,
    ) -> Result<RuntimeContentBase, RuntimeContentError> {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.directory.path()).unwrap(),
            &self.expected,
            recipe(),
            limits,
        )
    }

    fn remove_files(&self) {
        for path in [
            SOURCE_FILE,
            PRIMARY_FILE,
            AUXILIARY_FILE,
            STARTUP_FILE,
            LATER_FILE,
            RUNTIME_CONTENT_CATALOG_FILE,
        ] {
            fs::remove_file(self.directory.path().join(path)).unwrap();
        }
    }

    fn open_for_one_image(&self) -> RuntimeContentBase {
        self.open(
            RuntimeContentLimits::default()
                .with_retained_bytes(bytes(self.one_image_retained_limit())),
        )
        .unwrap()
    }

    /// Includes the captured base and one complete maximum codec/Host output reservation.
    fn one_image_retained_limit(&self) -> usize {
        let limits = RuntimeContentLimits::default()
            .with_retained_bytes(ImageRuntimeCodec::runtime_limits().retained_bytes());
        let probe = self.open(limits).unwrap();
        let required = match probe.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary()) {
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                required,
                ..
            }) => required,
            result => {
                panic!("expected retained-output reservation refusal before decode: {result:?}")
            }
        };
        drop(probe);
        let short = self
            .open(limits.with_retained_bytes(bytes(required - 1)))
            .unwrap();
        assert!(matches!(
            short.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary()),
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                ..
            })
        ));
        required
    }

    fn replace_primary(&mut self, encoded: &[u8], format: &str) {
        fs::write(self.directory.path().join(PRIMARY_FILE), encoded).unwrap();
        let members = self
            .catalog
            .members()
            .iter()
            .map(|member| {
                if member.path() != PRIMARY_FILE {
                    return member.clone();
                }
                RuntimeContentMember::new(
                    PRIMARY_FILE,
                    ContentDigest::of_bytes(encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(format).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    member.products().to_vec(),
                    member.dependencies().to_vec(),
                )
                .unwrap()
            })
            .collect();
        self.catalog = RuntimeContentCatalog::new(
            RuntimeContentCatalogDefinition {
                target: self.catalog.target().clone(),
                configuration: self.catalog.configuration().clone(),
                plugin_plan_fingerprint: self.catalog.plugin_plan_fingerprint(),
                schema_fingerprint: self.catalog.schema_fingerprint(),
                startup_scene: self.catalog.startup_scene(),
                scenes: self.catalog.scenes().to_vec(),
                addressable_roots: self.catalog.addressable_roots().to_vec(),
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = self.catalog.to_json_bytes().unwrap();
        fs::write(
            self.directory.path().join(RUNTIME_CONTENT_CATALOG_FILE),
            &encoded,
        )
        .unwrap();
        self.expected = RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target());
    }
}

thread_local! {
    static IMAGE_DECODE_CALLS: Cell<usize> = const { Cell::new(0) };
    static IMAGE_DECODE_GATE: RefCell<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>> =
        const { RefCell::new(None) };
}

/// Observes actual Image decoding without changing its format, promises or cost query.
struct ObservedImageCodec;

impl RuntimeAssetCodec<ImageAsset> for ObservedImageCodec {
    const RUNTIME_KIND: &'static str = ImageRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = ImageRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        ImageRuntimeCodec::runtime_limits()
    }

    fn encode_runtime(product: &ImageAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        ImageRuntimeCodec::encode_runtime(product)
    }

    fn decode_runtime(encoded: &[u8]) -> Result<ImageAsset, AssetRuntimeError> {
        IMAGE_DECODE_CALLS.set(IMAGE_DECODE_CALLS.get() + 1);
        let image = ImageRuntimeCodec::decode_runtime(encoded)?;
        if let Some((started, release)) = IMAGE_DECODE_GATE.with_borrow_mut(Option::take) {
            started.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        Ok(image)
    }

    fn runtime_retained_bytes(product: &ImageAsset) -> Result<usize, AssetRuntimeError> {
        ImageRuntimeCodec::runtime_retained_bytes(product)
    }

    fn visit_required_runtime_products<E>(
        product: &ImageAsset,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        ImageRuntimeCodec::visit_required_runtime_products(product, visitor)
    }

    fn contains_runtime_product(product: &ImageAsset, reference: AssetProductRef) -> bool {
        ImageRuntimeCodec::contains_runtime_product(product, reference)
    }
}

fn retained_image(value: &Arc<RuntimeContentAsset<ImageAsset>>) -> ImageAsset {
    value
        .product()
        .share_with_retention_owner(Arc::clone(value))
}

fn initial_world(value: &Arc<RuntimeContentAsset<ImageAsset>>) -> (World, Handle<ImageAsset>) {
    let mut server = AssetServer::new();
    let handle = server
        .reserve_product::<ImageAsset>(value.reference())
        .unwrap();
    let mut products = ImageProducts::default();
    products.publish(handle, retained_image(value));
    let mut world = World::new();
    world.insert_resource(server);
    world.insert_resource(products);
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    (world, handle)
}

fn resolve(world: &World, current: &SpawnedSceneInstance) -> Entity {
    match current.resolve(world, &portrait_id()) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live portrait: {other:?}"),
    }
}

fn assert_prepared(snapshot: &PreparedImageSnapshot, content: &ImageContentSnapshot) {
    assert_eq!(snapshot.identity().key().asset_id(), content.handle().id());
    assert_eq!(snapshot.identity().asset_version(), content.version());
    assert_eq!(snapshot.identity().slot_revision(), content.slot_revision());
    assert_eq!(
        snapshot.identity().content_hash(),
        RenderResourceContentHash::from_digest(content.content_hash().as_bytes())
    );
    assert_eq!(snapshot.resource().content_hash(), content.content_hash());
    assert_eq!(snapshot.resource().extent(), content.extent());
    assert_eq!(snapshot.resource().format(), content.format());
    assert_eq!(snapshot.resource().color_space(), content.color_space());
    assert_eq!(snapshot.resource().pixels(), content.pixels());
    assert!(snapshot.resource().has_retention_owner());
    assert!(std::ptr::eq(
        snapshot.resource().pixels().as_ptr(),
        content.pixels().as_ptr()
    ));
}

#[test]
fn imported_pixels_reach_source_free_native_scenes_and_prepared_packets_from_one_captured_base() {
    let fixture = Fixture::new();
    let base = fixture.open(RuntimeContentLimits::default()).unwrap();
    fixture.remove_files();
    let initial = base.acquire_startup_scene().unwrap();
    let next = base.acquire_scene(later()).unwrap();
    let primary_value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
            .unwrap(),
    );
    let auxiliary_value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary())
            .unwrap(),
    );
    assert!(primary_value.product().source().is_none());
    assert!(auxiliary_value.product().source().is_none());
    assert_eq!(primary_value.product().pixels(), fixture.primary_pixels);
    assert_eq!(primary_value.product().content_hash(), fixture.primary_hash);
    assert_eq!(primary_value.product().extent(), fixture.primary_extent);
    assert_eq!(primary_value.product().color_space(), ImageColorSpace::Srgb);
    assert_eq!(auxiliary_value.product().pixels(), AUXILIARY_PIXELS);
    assert_eq!(
        auxiliary_value.product().color_space(),
        ImageColorSpace::Linear
    );
    assert!(ImageDomain::contains_product(
        auxiliary_value.product(),
        auxiliary().product()
    ));
    assert!(!ImageDomain::contains_product(
        auxiliary_value.product(),
        ImportedProductId::PRIMARY
    ));

    let bindings = base.plan().schema_validation().bindings().clone();
    let (mut world, primary_handle) = initial_world(&primary_value);
    let auxiliary_handle = world
        .resource_mut::<AssetServer>()
        .reserve_product::<ImageAsset>(auxiliary())
        .unwrap();
    assert_ne!(primary_handle.id(), auxiliary_handle.id());
    world
        .resource_mut::<ImageProducts>()
        .publish(auxiliary_handle, retained_image(&auxiliary_value));
    for (handle, expected) in [(primary_handle, primary()), (auxiliary_handle, auxiliary())] {
        let server = world.resource::<AssetServer>();
        assert_eq!(server.product(handle.id()), Some(expected));
        assert!(server.path(handle.id()).is_none());
        let content = world.resource::<ImageProducts>().capture(&world, handle);
        assert_eq!(content.product(), expected);
        assert!(content.source().is_none());
        assert_prepared(&content.prepared_snapshot(), &content);
    }
    let current = bindings
        .prepare_runtime_scene(&mut world, initial.candidate(), &occurrence())
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    assert_eq!(
        world.get::<Portrait>(resolve(&world, &current)).unwrap().0,
        primary_handle
    );
    let current = bindings
        .prepare_runtime_scene_replacement(&mut world, next.candidate(), &current)
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    assert_eq!(
        world.get::<Portrait>(resolve(&world, &current)).unwrap().0,
        auxiliary_handle
    );
    let exported = bindings.export_scene(&world).into_output().unwrap();
    let reference =
        &exported.document.entities[0].components[&ComponentTypeId::new(PORTRAIT)].value;
    assert_eq!(
        decode_asset_reference_value(reference).unwrap(),
        AssetRef::Product(auxiliary())
    );
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();

    let mut encoded = ImageRuntimeCodec::encode_runtime(auxiliary_value.product()).unwrap();
    let roundtrip = ImageRuntimeCodec::decode_runtime(&encoded).unwrap();
    assert_eq!(
        roundtrip.content_hash(),
        auxiliary_value.product().content_hash()
    );
    assert!(roundtrip.source().is_none());
    *encoded.last_mut().unwrap() ^= 1;
    let changed = ImageRuntimeCodec::decode_runtime(&encoded).unwrap();
    assert_ne!(changed.content_hash(), roundtrip.content_hash());
}

fn replacement_products(
    world: &World,
    primary_handle: Handle<ImageAsset>,
    next_handle: Handle<ImageAsset>,
    next: &Arc<RuntimeContentAsset<ImageAsset>>,
) -> ImageProducts {
    let mut products = ImageProducts::default();
    let previous = world
        .resource::<ImageProducts>()
        .images
        .get(primary_handle)
        .unwrap();
    products.publish(primary_handle, previous.share_retained().unwrap());
    products.publish(next_handle, retained_image(next));
    products
}

#[test]
fn failed_and_canceled_replacements_keep_current_image_before_atomic_binding_value_scene_commit() {
    let fixture = Fixture::new();
    let base = fixture.open(RuntimeContentLimits::default()).unwrap();
    fixture.remove_files();
    let initial = base.acquire_startup_scene().unwrap();
    let next = base.acquire_scene(later()).unwrap();
    let bindings = base.plan().schema_validation().bindings().clone();
    let missing = processed_scene(&bindings, LATER_SOURCE, absent());
    let primary_value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
            .unwrap(),
    );
    let auxiliary_value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary())
            .unwrap(),
    );
    let (mut world, primary_handle) = initial_world(&primary_value);
    let current = bindings
        .prepare_runtime_scene(&mut world, initial.candidate(), &occurrence())
        .unwrap()
        .commit(&mut world)
        .instance
        .unwrap();
    let old = resolve(&world, &current);
    let identity = world.resource::<WorldIdentityDomain>().stats();
    let mut content = world
        .resource::<ImageProducts>()
        .capture(&world, primary_handle);
    let before = world.resource::<AssetServer>().clone();
    assert!(
        bindings
            .prepare_runtime_scene_replacement(&mut world, &missing, &current)
            .is_err()
    );
    assert!(
        bindings
            .prepare_runtime_scene_replacement(&mut world, next.candidate(), &current)
            .is_err()
    );
    assert_eq!(world.resource::<AssetServer>(), &before);
    world
        .resource_mut::<AssetServer>()
        .reserve_product::<String>(absent())
        .unwrap();
    let before = world.resource::<AssetServer>().clone();
    assert!(
        bindings
            .prepare_runtime_scene_replacement(&mut world, &missing, &current)
            .is_err()
    );

    let claims = SceneNativeRetirementClaims::default();
    let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE);
    let mut assets = SceneAssetBindings::from_world(&world);
    let rejected = assets.reserve_product::<ImageAsset>(auxiliary()).unwrap();
    let products = replacement_products(&world, primary_handle, rejected, &auxiliary_value);
    assert!(
        bindings
            .prepare_runtime_scene_replacement_with_claims(
                &mut world,
                &missing,
                &current,
                assets,
                &claims,
                limits,
                &[],
                |writer| {
                    writer.replace_resource(products);
                },
            )
            .is_err()
    );
    assert_eq!(world.resource::<AssetServer>(), &before);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
    assert_eq!(resolve(&world, &current), old);
    assert!(
        world
            .resource::<ImageProducts>()
            .images
            .get(rejected)
            .is_none()
    );
    for action in ["rewrite", "cancel", "commit"] {
        let mut assets = SceneAssetBindings::from_world(&world);
        let next_handle = assets.reserve_product::<ImageAsset>(auxiliary()).unwrap();
        let products = replacement_products(&world, primary_handle, next_handle, &auxiliary_value);
        let pending = bindings
            .prepare_runtime_scene_replacement_with_claims(
                &mut world,
                next.candidate(),
                &current,
                assets,
                &claims,
                limits,
                &[],
                |writer| {
                    writer.replace_resource(products);
                },
            )
            .unwrap_or_else(|report| panic!("Image product prepare failed: {report:?}"));
        assert_eq!(world.resource::<AssetServer>(), &before);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
        assert_eq!(resolve(&world, &current), old);
        let products = world.resource::<ImageProducts>();
        assert!(products.images.get(next_handle).is_none());
        assert!(content.is_current(
            world.resource::<AssetServer>(),
            &products.states,
            &products.images
        ));
        if action == "rewrite" {
            pending.validate_baseline(&mut world).unwrap();
            let ticks = world.get_resource_change_ticks::<ImageProducts>().unwrap();
            world
                .resource_mut::<ImageProducts>()
                .publish(primary_handle, retained_image(&primary_value));
            let products = world.resource::<ImageProducts>();
            let live_baseline = products.capture_baseline();
            assert_eq!(products.images.len(), 1);
            assert!(!content.is_current(
                world.resource::<AssetServer>(),
                &products.states,
                &products.images
            ));
            assert_eq!(
                world
                    .get_resource_change_ticks::<ImageProducts>()
                    .unwrap()
                    .changed,
                ticks.changed
            );
            let (pending, report) = match pending.commit(&mut world) {
                Err(failure) => failure,
                Ok(_) => panic!("equal-content Image republication must reject replacement"),
            };
            assert!(report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "scene.product-resource-changed"
            }));
            assert!(report.instance.is_none());
            assert_eq!(world.resource::<AssetServer>(), &before);
            assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identity);
            assert_eq!(resolve(&world, &current), old);
            let products = world.resource::<ImageProducts>();
            assert!(products.matches_baseline(&live_baseline));
            assert!(products.images.get(next_handle).is_none());
            pending
                .discard(&mut world)
                .unwrap_or_else(|_| panic!("stale Image product discard failed"));
            content = world
                .resource::<ImageProducts>()
                .capture(&world, primary_handle);
        } else if action == "cancel" {
            pending
                .discard(&mut world)
                .unwrap_or_else(|_| panic!("Image product discard failed"));
            assert_eq!(world.resource::<AssetServer>(), &before);
            assert_eq!(resolve(&world, &current), old);
        } else {
            let report = pending
                .commit(&mut world)
                .unwrap_or_else(|_| panic!("Image product commit failed"));
            assert!(!report.diagnostics.has_errors(), "{report:?}");
            let replacement = report.instance.unwrap();
            assert!(world.get_entity(old).is_err());
            assert_eq!(
                world
                    .get::<Portrait>(resolve(&world, &replacement))
                    .unwrap()
                    .0,
                next_handle
            );
            assert_eq!(
                world.resource::<AssetServer>().product(next_handle.id()),
                Some(auxiliary())
            );
            let next_content = world
                .resource::<ImageProducts>()
                .capture(&world, next_handle);
            assert_eq!(next_content.pixels(), AUXILIARY_PIXELS);
            assert_eq!(next_content.product(), auxiliary());
            assert_prepared(&next_content.prepared_snapshot(), &next_content);
            assert!(!world.contains_resource::<ProjectAssetDatabase>());
            retire_and_despawn_scene_instance(&mut world, &replacement).unwrap();
        }
    }
    assert_eq!(content.pixels(), fixture.primary_pixels);
}

#[test]
fn runtime_base_rejects_foreign_product_wrong_codec_corruption_and_missing_image_members() {
    let fixture = Fixture::new();
    let last_good = fixture.open(RuntimeContentLimits::default()).unwrap();
    let admitted = Arc::new(
        last_good
            .acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
            .unwrap(),
    );
    let (world, handle) = initial_world(&admitted);
    let content = world.resource::<ImageProducts>().capture(&world, handle);
    for case in [
        "wrong-product",
        "foreign-source",
        "wrong-codec",
        "truncated",
        "missing",
    ] {
        let mut candidate = Fixture::new();
        let reference = match case {
            "wrong-product" => auxiliary(),
            "foreign-source" => product(FOREIGN_SOURCE, ImportedProductId::PRIMARY),
            _ => primary(),
        };
        let image = ImageAsset::from_product(
            reference,
            ImageExtent::new(2, 1),
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            AUXILIARY_PIXELS.to_vec(),
        )
        .unwrap();
        let mut encoded = ImageRuntimeCodec::encode_runtime(&image)
            .unwrap()
            .into_vec();
        if case == "truncated" {
            encoded.pop();
        }
        let format = if case == "wrong-codec" {
            "unrelated_runtime_image"
        } else {
            ImageRuntimeCodec::RUNTIME_KIND
        };
        candidate.replace_primary(&encoded, format);
        if case == "missing" {
            fs::remove_file(candidate.directory.path().join(PRIMARY_FILE)).unwrap();
            assert!(matches!(
                candidate.open(RuntimeContentLimits::default()),
                Err(RuntimeContentError::Host(_))
            ));
        } else {
            let base = candidate.open(RuntimeContentLimits::default()).unwrap();
            let result = base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary());
            match case {
                "wrong-product" | "foreign-source" => assert!(matches!(
                    result,
                    Err(RuntimeContentError::ProductIdentityMismatch)
                )),
                "wrong-codec" => assert!(matches!(result, Err(RuntimeContentError::CodecMismatch))),
                "truncated" => assert!(matches!(result, Err(RuntimeContentError::Asset(_)))),
                _ => unreachable!(),
            }
        }
        let products = world.resource::<ImageProducts>();
        assert!(content.is_current(
            world.resource::<AssetServer>(),
            &products.states,
            &products.images
        ));
        assert_eq!(content.pixels(), fixture.primary_pixels);
        assert_eq!(
            world.resource::<AssetServer>().product(handle.id()),
            Some(primary())
        );
    }
    assert!(matches!(
        last_good.acquire_asset::<ImageDomain, ImageRuntimeCodec>(absent()),
        Err(RuntimeContentError::UndeclaredProduct)
    ));
}

#[test]
fn many_independent_small_images_refund_maximum_output_until_the_last_shared_owner() {
    const COUNT: usize = 32;

    let fixture = Fixture::small_images();
    let one_image = fixture.one_image_retained_limit();
    let probe = fixture
        .open(RuntimeContentLimits::default().with_retained_bytes(bytes(one_image)))
        .unwrap();
    let first = probe
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
        .unwrap();
    let actual = first.retained_bytes();
    let product_bytes = ImageRuntimeCodec::runtime_retained_bytes(first.product()).unwrap();
    assert!(product_bytes >= size_of::<ImageAsset>() + fixture.primary_pixels.len());
    assert!(
        actual > product_bytes,
        "the Host wrapper must also be charged"
    );
    let maximum =
        ImageRuntimeCodec::runtime_limits().retained_bytes().get() + (actual - product_bytes);
    assert!(COUNT * actual < maximum);
    drop((first, probe));

    // One base + one maximum decode + the actual charges of the preceding outputs.
    // This is smaller than the old charge for even two maximum-sized outputs.
    let cap = one_image + (COUNT - 1) * actual;
    assert!(cap < one_image + maximum);
    let base = fixture
        .open(RuntimeContentLimits::default().with_retained_bytes(bytes(cap)))
        .unwrap();
    fixture.remove_files();
    let calls = IMAGE_DECODE_CALLS.get();
    let mut outputs: Vec<Arc<RuntimeContentAsset<ImageAsset>>> = Vec::new();
    for index in 0..COUNT {
        let reference = if index % 2 == 0 {
            primary()
        } else {
            auxiliary()
        };
        let value = Arc::new(
            base.acquire_asset::<ImageDomain, ObservedImageCodec>(reference)
                .unwrap(),
        );
        assert_eq!(value.reference(), reference);
        assert_eq!(value.product().product(), Some(reference));
        assert!(value.product().source().is_none());
        assert_eq!(value.retained_bytes(), actual);
        assert!(std::ptr::eq(value.base().catalog(), base.catalog()));
        assert!(std::ptr::eq(value.base().plan(), base.plan()));
        assert_eq!(
            value.product().pixels(),
            if reference == primary() {
                fixture.primary_pixels.as_slice()
            } else {
                AUXILIARY_PIXELS
            }
        );
        for previous in &outputs {
            assert!(!std::ptr::eq(
                previous.product().pixels().as_ptr(),
                value.product().pixels().as_ptr()
            ));
        }
        outputs.push(value);
    }
    assert_eq!(IMAGE_DECODE_CALLS.get(), calls + COUNT);

    let assert_full = || {
        let calls = IMAGE_DECODE_CALLS.get();
        let result = base.acquire_asset::<ImageDomain, ObservedImageCodec>(primary());
        assert!(matches!(
            result,
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                required,
                limit,
            }) if required == cap + actual && limit == cap
        ));
        assert_eq!(IMAGE_DECODE_CALLS.get(), calls, "refused before decode");
    };
    assert_full();

    let value = outputs.remove(0);
    let owner = Arc::downgrade(&value);
    let shared = value.as_ref().clone();
    assert_eq!(shared.retained_bytes(), actual);
    assert!(std::ptr::eq(
        shared.product().pixels().as_ptr(),
        value.product().pixels().as_ptr()
    ));
    let (world, handle) = initial_world(&value);
    let content = world.resource::<ImageProducts>().capture(&world, handle);
    let prepared = content.prepared_snapshot();
    assert_prepared(&prepared, &content);
    let prepared_clone = prepared.clone();
    drop((world, content, value));
    assert!(owner.upgrade().is_some());
    assert_full();
    drop(prepared);
    assert_eq!(prepared_clone.resource().pixels(), fixture.primary_pixels);
    assert!(owner.upgrade().is_some());
    assert_full();
    drop(prepared_clone);
    assert!(owner.upgrade().is_none());
    assert_eq!(shared.product().pixels(), fixture.primary_pixels);
    assert_full();
    drop(shared);

    let recovered = Arc::new(
        base.acquire_asset::<ImageDomain, ObservedImageCodec>(primary())
            .unwrap(),
    );
    assert_eq!(recovered.retained_bytes(), actual);
    assert_eq!(recovered.product().pixels(), fixture.primary_pixels);
    for remaining in &outputs {
        assert!(!std::ptr::eq(
            remaining.product().pixels().as_ptr(),
            recovered.product().pixels().as_ptr()
        ));
    }
    outputs.push(recovered);
    assert_eq!(outputs.len(), COUNT);
    assert_eq!(IMAGE_DECODE_CALLS.get(), calls + COUNT + 1);
    assert_full();
}

#[test]
fn held_image_decode_keeps_full_retained_and_work_promises_until_success() {
    let fixture = Fixture::with_primary_image(
        ImageAsset::from_product(
            primary(),
            ImageExtent::new(1024, 1024),
            ImageFormat::Rgba8,
            ImageColorSpace::Srgb,
            vec![37; 4 * 1024 * 1024],
        )
        .unwrap(),
    );
    assert_eq!(fixture.primary_pixels.len(), 4 * 1024 * 1024);
    let one_image = fixture.one_image_retained_limit();
    let probe = fixture
        .open(RuntimeContentLimits::default().with_retained_bytes(bytes(one_image)))
        .unwrap();
    let value = probe
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
        .unwrap();
    let actual = value.retained_bytes();
    let product_bytes = ImageRuntimeCodec::runtime_retained_bytes(value.product()).unwrap();
    let codec = ImageRuntimeCodec::runtime_limits();
    let maximum = codec.retained_bytes().get() + (actual - product_bytes);
    assert!(actual < maximum);
    drop((value, probe));

    let retained_cap = one_image + actual;
    let cases = [
        (
            fixture
                .open(RuntimeContentLimits::default().with_retained_bytes(bytes(retained_cap)))
                .unwrap(),
            RuntimeContentBudgetKind::RetainedBytes,
            one_image + maximum,
            retained_cap,
        ),
        (
            fixture
                .open(
                    RuntimeContentLimits::default()
                        .with_retained_bytes(bytes(one_image + maximum))
                        .with_work_bytes(codec.work_bytes()),
                )
                .unwrap(),
            RuntimeContentBudgetKind::WorkBytes,
            2 * codec.work_bytes().get(),
            codec.work_bytes().get(),
        ),
    ];
    fixture.remove_files();
    for (base, kind, required, limit) in cases {
        let (started, observe) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let first = thread::scope(|scope| {
            let base = &base;
            let held = scope.spawn(move || {
                IMAGE_DECODE_GATE.with_borrow_mut(|gate| {
                    assert!(gate.is_none());
                    *gate = Some((started, blocked));
                });
                base.acquire_asset::<ImageDomain, ObservedImageCodec>(primary())
            });
            observe.recv_timeout(Duration::from_secs(10)).unwrap();
            let calls = IMAGE_DECODE_CALLS.get();
            let refused = base.acquire_asset::<ImageDomain, ObservedImageCodec>(auxiliary());
            let after = IMAGE_DECODE_CALLS.get();
            release.send(()).unwrap();
            let first = held.join().unwrap().unwrap();
            assert!(matches!(
                refused,
                Err(RuntimeContentError::Budget {
                    kind: refused_kind,
                    required: refused_required,
                    limit: refused_limit,
                }) if (refused_kind, refused_required, refused_limit) == (kind, required, limit)
            ));
            assert_eq!(
                after, calls,
                "overlapping decode refused before codec entry"
            );
            first
        });
        assert_eq!(first.retained_bytes(), actual);
        assert_eq!(first.product().pixels(), fixture.primary_pixels);
        let second = base
            .acquire_asset::<ImageDomain, ObservedImageCodec>(auxiliary())
            .unwrap();
        assert!(second.retained_bytes() < actual);
        assert_eq!(second.product().pixels(), AUXILIARY_PIXELS);
        assert!(!std::ptr::eq(
            first.product().pixels().as_ptr(),
            second.product().pixels().as_ptr()
        ));
    }
}

#[test]
fn escaped_cpu_and_prepared_captures_keep_decoded_output_charged_until_the_last_pixel_owner() {
    let fixture = Fixture::new();
    let base = fixture.open_for_one_image();
    fixture.remove_files();
    let value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(primary())
            .unwrap(),
    );
    let owner = Arc::downgrade(&value);
    let (mut world, handle) = initial_world(&value);
    let content = world.resource::<ImageProducts>().capture(&world, handle);
    assert!(std::ptr::eq(
        content.pixels().as_ptr(),
        value.product().pixels().as_ptr()
    ));
    let prepared = content.prepared_snapshot();
    assert_prepared(&prepared, &content);
    let prepared_clone = prepared.clone();
    let retry_base = base.clone();
    drop(base);
    drop(value);
    let ImageProducts {
        mut images,
        mut states,
        mut events,
        ..
    } = world.remove_resource::<ImageProducts>().unwrap();
    drop(
        images
            .remove_with_state(handle, &mut states, &mut events)
            .unwrap()
            .unwrap(),
    );
    assert!(!content.is_authorized(world.resource::<AssetServer>(), &states, &images));
    drop((images, states, events));
    drop(world);
    assert!(owner.upgrade().is_some());
    assert!(matches!(
        retry_base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary()),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        })
    ));
    drop(content);
    assert_eq!(prepared.resource().pixels(), fixture.primary_pixels);
    assert!(matches!(
        retry_base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary()),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        })
    ));
    drop(prepared);
    assert_eq!(prepared_clone.resource().pixels(), fixture.primary_pixels);
    assert!(owner.upgrade().is_some());
    assert!(matches!(
        retry_base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary()),
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            ..
        })
    ));
    drop(prepared_clone);
    assert!(owner.upgrade().is_none());
    let recovered = retry_base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(auxiliary())
        .unwrap();
    assert_eq!(recovered.product().pixels(), AUXILIARY_PIXELS);
    assert_eq!(recovered.product().product(), Some(auxiliary()));
}
