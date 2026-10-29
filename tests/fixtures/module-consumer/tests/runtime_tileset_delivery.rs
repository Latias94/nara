//! Independent processed TileSet delivery through the public Runtime and Scene owners.
//!
//! This consumer owns its concrete Image/TileSet aggregate. It does not qualify stock renderer
//! resource routing, a packaged Player, GPU upload, or source-backed asset discovery.

use std::{fs, sync::Arc};

use nara_asset::{
    AssetError, AssetEvents, AssetPath, AssetProductRef, AssetRecord, AssetRef, AssetServer,
    AssetSlotRevision, AssetSourceKind, AssetStateRevision, AssetStates, Assets, Handle,
    ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, Color, FormatKind, FormatVersion, ItemLimit, Vec2};
use nara_ecs::{Component, Entity, Resource, World};
use nara_engine::{
    ProductRecipe,
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
use nara_fs::ContentDigest;
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageContentSnapshot, ImageDomain,
    ImageExtent, ImageFormat, ImageImportSettings, ImageImporter, ImageRuntimeCodec,
};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_reflect::{
    ComponentCodecError, ComponentTypeId, ComponentValue, decode_asset_reference_value,
    encode_asset_reference_value,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits,
    SceneComponentBindings, SceneEntityId, SceneOccurrenceKey, SourceDocumentKey,
    SourceDocumentKind,
    advanced::{
        PreparedSceneReplacement, SceneAssetBindings, SceneNativeRetirementClaims,
        SceneProductResource, SceneProductTransactionLimits,
    },
    retire_and_despawn_scene_instance,
};
use nara_tilemap::{
    TileAtlasLayout, TileCoord, TileIndex, TileSet, TileSetAssetError, TileSetAssets,
    TileSetContentReceipt, TileSetDescriptor, TileSetDomain, TileSetPublicationInput,
    TileSetRuntimeCodec, Tilemap, TilemapMembershipError, validate_persisted_tilemap_membership,
};
use serde_json::json;

const IMAGE_SOURCE: &str = "504424e8-bebb-456f-96e0-6f373d861283";
const TILESET_SOURCE: &str = "b41c65a7-5800-4793-8c81-c4eefc0f0d62";
const AUXILIARY: &str = "a5a0cd22-2b7f-4f08-810a-bd34d16f9b4a";
const STARTUP_SOURCE: &str = "e1034378-3bbe-4198-8003-f4afbe747dd4";
const LATER_SOURCE: &str = "1cb053a7-c717-42a8-bf94-888d655d985f";
const UNRELATED_SOURCE: &str = "92b91d28-42f2-4bb5-bf92-9e985959f4ee";
const TILEMAP: &str = "nara.tilemap.Tilemap";
const SOURCE_FILE: &str = "authoring.png";
const PRIMARY_IMAGE_FILE: &str = "primary.runtime.image";
const AUXILIARY_IMAGE_FILE: &str = "auxiliary.runtime.image";
const PRIMARY_TILESET_FILE: &str = "primary.runtime.tileset";
const AUXILIARY_TILESET_FILE: &str = "auxiliary.runtime.tileset";
const STARTUP_FILE: &str = "startup.runtime.json";
const LATER_FILE: &str = "later.runtime.json";
const SOURCE_PNG: &[u8] =
    include_bytes!("../../../../samples/two-level-game/assets/textures/meadow.png");

fn stable(value: &str) -> StableAssetId {
    StableAssetId::parse_str(value).unwrap()
}

fn product(source: &str, auxiliary: bool) -> AssetProductRef {
    AssetProductRef::new(
        stable(source),
        if auxiliary {
            ImportedProductId::parse_subproduct(AUXILIARY).unwrap()
        } else {
            ImportedProductId::PRIMARY
        },
    )
    .unwrap()
}

fn image_product(auxiliary: bool) -> AssetProductRef {
    product(IMAGE_SOURCE, auxiliary)
}

fn tileset_product(auxiliary: bool) -> AssetProductRef {
    product(TILESET_SOURCE, auxiliary)
}

fn source_key(source: &str) -> SourceDocumentKey {
    SourceDocumentKey::stable_asset(stable(source), SourceDocumentKind::Scene)
}

fn tilemap_id() -> SceneEntityId {
    SceneEntityId::new("terrain").unwrap()
}

fn occurrence() -> SceneOccurrenceKey {
    SceneOccurrenceKey::new("consumer.runtime-tileset", "current").unwrap()
}

fn recipe() -> ProductRecipe {
    // The selected Runtime2d plan supplies the existing TILEMAP_SCHEMA_PROVIDER.
    ProductRecipe::new()
}

fn target() -> RuntimeContentTarget {
    RuntimeContentTarget::new("x86_64-pc-windows-msvc", "runtime-tileset-consumer").unwrap()
}

fn tilemap_value(reference: AssetProductRef, tile: u32) -> ComponentValue {
    ComponentValue::map([
        (
            "tile_size",
            ComponentValue::map([
                ("x", ComponentValue::f64(1.0).unwrap()),
                ("y", ComponentValue::f64(1.0).unwrap()),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("y_sort_anchor", ComponentValue::Null),
        (
            "tileset",
            encode_asset_reference_value(&AssetRef::Product(reference)),
        ),
        (
            "cells",
            ComponentValue::List(vec![ComponentValue::map([
                (
                    "coord",
                    ComponentValue::map([
                        ("x", ComponentValue::I64(2)),
                        ("y", ComponentValue::I64(-3)),
                    ]),
                ),
                (
                    "cell",
                    ComponentValue::map([
                        ("tile", ComponentValue::U64(u64::from(tile))),
                        (
                            "color",
                            ComponentValue::map([
                                ("r", ComponentValue::f64(1.0).unwrap()),
                                ("g", ComponentValue::f64(1.0).unwrap()),
                                ("b", ComponentValue::f64(1.0).unwrap()),
                                ("a", ComponentValue::f64(1.0).unwrap()),
                            ]),
                        ),
                    ]),
                ),
            ])]),
        ),
    ])
}

/// Uses the actual public processed format, with canonical Product leaves and no source database.
fn processed_scene(
    bindings: &SceneComponentBindings,
    source: &str,
    reference: AssetProductRef,
    tile: u32,
) -> RuntimeSceneCandidate {
    let encoded = serde_json::to_vec(&json!({
        "kind": "runtime_scene", "format_version": 1, "engine_min_version": "0.1.0",
        "generator": {"name": "runtime-tileset-consumer", "version": "0.0.0"},
        "payload": {
            "source_key": source_key(source),
            "roots": ["terrain"],
            "entities": [{"id": "terrain", "children": [], "components": {
                (TILEMAP): {"version": 1, "value": tilemap_value(reference, tile)}
            }}],
            "provenance": [{"entity": "terrain", "local_entity": "terrain",
                "origin": {"kind": "local"}}]
        }
    }))
    .unwrap();
    RuntimeSceneDocumentCandidate::from_json_bytes(&encoded, RuntimeSceneLimits::default())
        .unwrap()
        .admit(&source_key(source), bindings)
        .unwrap()
}

fn validate_cells(
    candidate: &RuntimeSceneCandidate,
    descriptor: TileSetDescriptor,
) -> Result<(), TilemapMembershipError> {
    let value = &candidate.document().entities[0].components[&ComponentTypeId::new(TILEMAP)].value;
    validate_persisted_tilemap_membership(value, |reference| {
        (reference == &AssetRef::Product(descriptor.product())).then_some(descriptor.atlas())
    })
}

#[derive(Clone, Copy)]
enum FixtureCase {
    Valid,
    OmittedDependency,
    SoftDependency,
    WrongImageDependency,
    MissingImageMember,
    WrongImageFamily,
    WrongImageExtent,
    OutsideAtlas,
}

struct Fixture {
    directory: tempfile::TempDir,
    expected: RuntimeContentExpectation,
    primary_pixels: Vec<u8>,
    auxiliary_pixels: Vec<u8>,
    extent: ImageExtent,
}

impl Fixture {
    fn new() -> Self {
        Self::with_case(FixtureCase::Valid)
    }

    fn with_case(case: FixtureCase) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(SOURCE_FILE), SOURCE_PNG).unwrap();
        // This is the real producer import. Processed publication below never invents source facts.
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
        assert_eq!(imported.product(), Some(image_product(false)));
        let extent = imported.extent();
        let primary_pixels = imported.pixels().to_vec();
        let primary_image = ImageRuntimeCodec::encode_runtime(&imported).unwrap();
        let atlas = TileAtlasLayout::grid(Vec2::ONE, extent.width, extent.height);
        assert!(atlas.contains_tile(TileIndex::new(1)));
        let auxiliary_extent = if matches!(case, FixtureCase::WrongImageExtent) {
            ImageExtent::new(extent.width + 1, extent.height)
        } else {
            extent
        };
        let auxiliary_pixels =
            (0..auxiliary_extent.width as usize * auxiliary_extent.height as usize * 4)
                .map(|index| primary_pixels[index % primary_pixels.len()].wrapping_add(17))
                .collect::<Vec<_>>();
        let auxiliary_image = ImageAsset::from_product(
            image_product(true),
            auxiliary_extent,
            ImageFormat::Rgba8,
            ImageColorSpace::Linear,
            auxiliary_pixels.clone(),
        )
        .unwrap();
        let auxiliary_image = ImageRuntimeCodec::encode_runtime(&auxiliary_image).unwrap();
        drop(imported);

        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan =
            resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe()).unwrap();
        let mut members = Vec::new();
        for (path, source, reference, tile) in [
            (STARTUP_FILE, STARTUP_SOURCE, tileset_product(false), 0),
            (
                LATER_FILE,
                LATER_SOURCE,
                tileset_product(true),
                if matches!(case, FixtureCase::OutsideAtlas) {
                    extent.width * extent.height
                } else {
                    1
                },
            ),
        ] {
            let encoded =
                processed_scene(plan.schema_validation().bindings(), source, reference, tile)
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
                    vec![product(source, false)],
                    vec![RuntimeContentDependency::new(
                        reference,
                        RuntimeContentDependencyRole::Required,
                    )],
                )
                .unwrap(),
            );
        }
        for auxiliary in [false, true] {
            let descriptor =
                TileSetDescriptor::new(tileset_product(auxiliary), image_product(auxiliary), atlas)
                    .unwrap()
                    .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
                    .with_alpha_mode(AlphaMode2d::Opaque)
                    .with_tint(if auxiliary {
                        Color::rgb(0.25, 0.5, 0.75)
                    } else {
                        Color::WHITE
                    })
                    .unwrap();
            let dependencies = if !auxiliary {
                vec![RuntimeContentDependency::new(
                    image_product(false),
                    RuntimeContentDependencyRole::Required,
                )]
            } else {
                match case {
                    FixtureCase::OmittedDependency => vec![],
                    FixtureCase::SoftDependency | FixtureCase::MissingImageMember => {
                        vec![RuntimeContentDependency::new(
                            image_product(true),
                            RuntimeContentDependencyRole::Soft,
                        )]
                    }
                    FixtureCase::WrongImageDependency => vec![RuntimeContentDependency::new(
                        image_product(false),
                        RuntimeContentDependencyRole::Required,
                    )],
                    _ => vec![RuntimeContentDependency::new(
                        image_product(true),
                        RuntimeContentDependencyRole::Required,
                    )],
                }
            };
            let encoded = TileSetRuntimeCodec::encode_runtime(&descriptor).unwrap();
            let path = if auxiliary {
                AUXILIARY_TILESET_FILE
            } else {
                PRIMARY_TILESET_FILE
            };
            fs::write(directory.path().join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(TileSetRuntimeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![descriptor.product()],
                    dependencies,
                )
                .unwrap(),
            );
        }
        for (auxiliary, path, mut encoded) in [
            (false, PRIMARY_IMAGE_FILE, primary_image),
            (true, AUXILIARY_IMAGE_FILE, auxiliary_image),
        ] {
            if auxiliary && matches!(case, FixtureCase::MissingImageMember) {
                continue;
            }
            let wrong_family = auxiliary && matches!(case, FixtureCase::WrongImageFamily);
            let format = if wrong_family {
                encoded = TileSetRuntimeCodec::encode_runtime(
                    &TileSetDescriptor::new(image_product(true), image_product(false), atlas)
                        .unwrap(),
                )
                .unwrap();
                TileSetRuntimeCodec::RUNTIME_KIND
            } else {
                ImageRuntimeCodec::RUNTIME_KIND
            };
            fs::write(directory.path().join(path), &encoded).unwrap();
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(format).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![image_product(auxiliary)],
                    if wrong_family {
                        vec![RuntimeContentDependency::new(
                            image_product(false),
                            RuntimeContentDependencyRole::Required,
                        )]
                    } else {
                        vec![]
                    },
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
                startup_scene: product(STARTUP_SOURCE, false),
                scenes: vec![product(STARTUP_SOURCE, false), product(LATER_SOURCE, false)],
                addressable_roots: vec![tileset_product(false), tileset_product(true)],
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
        // The runtime must work even before any acquisition has a chance to consult authoring data.
        fs::remove_file(directory.path().join(SOURCE_FILE)).unwrap();
        Self {
            directory,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target()),
            primary_pixels,
            auxiliary_pixels,
            extent,
        }
    }

    fn open(&self, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.directory.path()).unwrap(),
            &self.expected,
            recipe(),
            limits,
        )
        .unwrap()
    }

    fn capture(&self) -> RuntimeContentBase {
        let base = self.open(RuntimeContentLimits::default());
        self.remove_outputs();
        base
    }

    fn remove_outputs(&self) {
        assert!(!self.directory.path().join(SOURCE_FILE).exists());
        for path in [
            PRIMARY_IMAGE_FILE,
            AUXILIARY_IMAGE_FILE,
            PRIMARY_TILESET_FILE,
            AUXILIARY_TILESET_FILE,
            STARTUP_FILE,
            LATER_FILE,
            RUNTIME_CONTENT_CATALOG_FILE,
        ] {
            let path = self.directory.path().join(path);
            if path.exists() {
                fs::remove_file(path).unwrap();
            }
        }
    }

    fn capture_for_one_image(&self) -> RuntimeContentBase {
        let limits = RuntimeContentLimits::default()
            .with_retained_bytes(ImageRuntimeCodec::runtime_limits().retained_bytes());
        let probe = self.open(limits);
        let required =
            match probe.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(false)) {
                Err(RuntimeContentError::Budget {
                    kind: RuntimeContentBudgetKind::RetainedBytes,
                    required,
                    ..
                }) => required,
                result => panic!("expected one maximum Image reservation refusal: {result:?}"),
            };
        drop(probe);
        let base = self.open(limits.with_retained_bytes(ByteLimit::new(required).unwrap()));
        self.remove_outputs();
        base
    }
}

#[derive(Clone, Copy)]
struct PairHandles {
    tileset: Handle<TileSet>,
    image: Handle<ImageAsset>,
}

impl PairHandles {
    fn reserve(bindings: &mut SceneAssetBindings, descriptor: TileSetDescriptor) -> Self {
        Self {
            tileset: bindings
                .reserve_product::<TileSet>(descriptor.product())
                .unwrap(),
            image: bindings
                .reserve_product::<ImageAsset>(descriptor.image_product())
                .unwrap(),
        }
    }
}

/// Private, complete resource generation used only by this independent consumer.
/// Every write goes through the methods below; the generation also fences owner/event changes.
#[derive(Resource)]
struct TileProducts {
    images: Assets<ImageAsset>,
    tilesets: Assets<TileSet>,
    states: AssetStates,
    events: AssetEvents,
    owner: TileSetAssets,
    incarnation: Arc<()>,
    generation: u64,
    image_handles: Vec<Handle<ImageAsset>>,
    tileset_handles: Vec<Handle<TileSet>>,
}

impl Default for TileProducts {
    fn default() -> Self {
        Self {
            images: Assets::default(),
            tilesets: Assets::default(),
            states: AssetStates::default(),
            events: AssetEvents::default(),
            owner: TileSetAssets::default(),
            incarnation: Arc::new(()),
            generation: 0,
            image_handles: Vec::new(),
            tileset_handles: Vec::new(),
        }
    }
}

struct ProductBaseline {
    incarnation: Arc<()>,
    generation: u64,
    images: Vec<(Handle<ImageAsset>, AssetSlotRevision, AssetStateRevision)>,
    tilesets: Vec<(Handle<TileSet>, AssetSlotRevision, AssetStateRevision)>,
}

impl SceneProductResource for TileProducts {
    type Baseline = ProductBaseline;

    fn capture_baseline(&self) -> Self::Baseline {
        ProductBaseline {
            incarnation: Arc::clone(&self.incarnation),
            generation: self.generation,
            images: self
                .image_handles
                .iter()
                .map(|handle| {
                    (
                        *handle,
                        self.images.slot_revision(*handle),
                        self.states.state_revision(handle.id()),
                    )
                })
                .collect(),
            tilesets: self
                .tileset_handles
                .iter()
                .map(|handle| {
                    (
                        *handle,
                        self.tilesets.slot_revision(*handle),
                        self.states.state_revision(handle.id()),
                    )
                })
                .collect(),
        }
    }

    fn matches_baseline(&self, baseline: &Self::Baseline) -> bool {
        Arc::ptr_eq(&self.incarnation, &baseline.incarnation)
            && self.generation == baseline.generation
            && baseline.images.iter().all(|(handle, slot, state)| {
                self.images.slot_revision(*handle) == *slot
                    && self.states.state_revision(handle.id()) == *state
            })
            && baseline.tilesets.iter().all(|(handle, slot, state)| {
                self.tilesets.slot_revision(*handle) == *slot
                    && self.states.state_revision(handle.id()) == *state
            })
    }
}

impl TileProducts {
    fn advance(&mut self) {
        self.generation = self.generation.checked_add(1).unwrap();
    }

    fn publish(
        &mut self,
        server: &AssetServer,
        handles: PairHandles,
        descriptor: TileSetDescriptor,
        image: ImageAsset,
        receipt: Option<TileSetContentReceipt>,
        retirements: &[TileSetContentReceipt],
    ) -> Result<TileSetContentReceipt, TileSetAssetError> {
        let input = TileSetPublicationInput::from_product(
            handles.tileset,
            descriptor,
            handles.image,
            image.share_retained().unwrap(),
        );
        let input = match receipt {
            Some(receipt) => input.with_receipt(receipt),
            None => input,
        };
        let prepared = self.owner.prepare_publications(
            vec![input],
            retirements,
            server,
            None,
            &self.tilesets,
            &self.states,
        )?;
        self.images
            .commit_loaded(
                handles.image,
                image,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .map_err(TileSetAssetError::State)?;
        // Consume the exclusive owner borrow before this aggregate can move into the Scene overlay.
        let mut receipts = prepared.commit(
            server,
            &self.images,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        );
        if !self.image_handles.contains(&handles.image) {
            self.image_handles.push(handles.image);
        }
        if !self.tileset_handles.contains(&handles.tileset) {
            self.tileset_handles.push(handles.tileset);
        }
        self.advance();
        Ok(receipts.pop().unwrap())
    }

    fn receipt(&self, handle: Handle<TileSet>) -> TileSetContentReceipt {
        self.owner
            .content_receipt(handle, &self.tilesets, &self.states)
            .unwrap()
    }

    fn retire(
        &mut self,
        server: &AssetServer,
        receipts: &[TileSetContentReceipt],
    ) -> Result<(), TileSetAssetError> {
        self.owner
            .prepare_publications(vec![], receipts, server, None, &self.tilesets, &self.states)?
            .commit(
                server,
                &self.images,
                &mut self.tilesets,
                &mut self.states,
                &mut self.events,
            );
        self.advance();
        Ok(())
    }

    fn remove_image(&mut self, handle: Handle<ImageAsset>) {
        self.images
            .remove_with_state(handle, &mut self.states, &mut self.events)
            .unwrap();
        self.advance();
    }

    fn republish_image(&mut self, handle: Handle<ImageAsset>, image: ImageAsset) {
        self.images
            .commit_loaded(
                handle,
                image,
                &mut self.states,
                &mut self.events,
                None,
                None,
            )
            .unwrap();
        self.advance();
    }

    fn reconcile(&mut self, server: &mut AssetServer) {
        self.owner.reconcile(
            server,
            None,
            &self.images,
            &mut self.tilesets,
            &mut self.states,
            &mut self.events,
        );
        self.advance();
    }

    fn capture_image(
        &self,
        server: &AssetServer,
        handle: Handle<ImageAsset>,
    ) -> ImageContentSnapshot {
        ImageContentSnapshot::capture(handle, server, &self.states, &self.images).unwrap()
    }
}

fn acquire_descriptor(base: &RuntimeContentBase, auxiliary: bool) -> TileSetDescriptor {
    let value = base
        .acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(tileset_product(auxiliary))
        .unwrap();
    assert!(std::ptr::eq(value.base().catalog(), base.catalog()));
    assert!(std::ptr::eq(value.base().plan(), base.plan()));
    assert_eq!(value.reference(), tileset_product(auxiliary));
    assert_eq!(value.product().product(), value.reference());
    *value.product()
}

fn acquire_image(
    base: &RuntimeContentBase,
    auxiliary: bool,
) -> Arc<RuntimeContentAsset<ImageAsset>> {
    let value = Arc::new(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(auxiliary))
            .unwrap(),
    );
    assert!(std::ptr::eq(value.base().catalog(), base.catalog()));
    assert!(std::ptr::eq(value.base().plan(), base.plan()));
    assert!(value.product().source().is_none());
    value
}

fn retained_image(value: &Arc<RuntimeContentAsset<ImageAsset>>) -> ImageAsset {
    value
        .product()
        .share_with_retention_owner(Arc::clone(value))
}

fn initial_world(base: &RuntimeContentBase) -> (World, SpawnedSceneInstance, PairHandles) {
    let initial = base.acquire_startup_scene().unwrap();
    let descriptor = acquire_descriptor(base, false);
    validate_cells(initial.candidate(), descriptor).unwrap();
    let image = acquire_image(base, false);
    let mut server = AssetServer::new();
    let handles = PairHandles {
        tileset: server
            .reserve_product::<TileSet>(descriptor.product())
            .unwrap(),
        image: server
            .reserve_product::<ImageAsset>(descriptor.image_product())
            .unwrap(),
    };
    let mut products = TileProducts::default();
    products
        .publish(
            &server,
            handles,
            descriptor,
            retained_image(&image),
            None,
            &[],
        )
        .unwrap();
    let mut world = World::new();
    world.insert_resource(server);
    world.insert_resource(products);
    let report = base
        .plan()
        .schema_validation()
        .bindings()
        .prepare_runtime_scene(&mut world, initial.candidate(), &occurrence())
        .unwrap()
        .commit(&mut world);
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    (world, report.instance.unwrap(), handles)
}

fn resolve(world: &World, current: &SpawnedSceneInstance) -> Entity {
    match current.resolve(world, &tilemap_id()) {
        EntityLookup::Resolved(entity) => entity,
        other => panic!("expected a live native Tilemap: {other:?}"),
    }
}

/// Rebuilds the consumer's private complete generation with its own receipts. Live receipts are
/// never transferred to another TileSet owner. Only the final Scene transaction publishes it.
fn replacement_products(
    world: &World,
    staged_server: &AssetServer,
    previous: PairHandles,
    previous_descriptor: TileSetDescriptor,
    next: PairHandles,
    descriptor: TileSetDescriptor,
    image: &Arc<RuntimeContentAsset<ImageAsset>>,
) -> TileProducts {
    let mut products = TileProducts::default();
    let previous_image = world
        .resource::<TileProducts>()
        .images
        .get(previous.image)
        .unwrap()
        .share_retained()
        .unwrap();
    let retirement = products
        .publish(
            staged_server,
            previous,
            previous_descriptor,
            previous_image,
            None,
            &[],
        )
        .unwrap();
    products
        .publish(
            staged_server,
            next,
            descriptor,
            retained_image(image),
            None,
            &[retirement],
        )
        .unwrap();
    products.remove_image(previous.image);
    assert!(products.tilesets.get(previous.tileset).is_none());
    assert!(products.images.get(previous.image).is_none());
    assert_eq!(products.owner.len(), 1);
    products
}

fn prepare_next(
    base: &RuntimeContentBase,
    world: &mut World,
    current: &SpawnedSceneInstance,
    previous: PairHandles,
    claims: &SceneNativeRetirementClaims,
) -> (PreparedSceneReplacement, PairHandles) {
    let scene = base.acquire_scene(product(LATER_SOURCE, false)).unwrap();
    let descriptor = acquire_descriptor(base, true);
    validate_cells(scene.candidate(), descriptor).unwrap();
    let image = acquire_image(base, true);
    let mut assets = SceneAssetBindings::from_world(world);
    let handles = PairHandles::reserve(&mut assets, descriptor);
    let products = replacement_products(
        world,
        assets.staged_server(),
        previous,
        acquire_descriptor(base, false),
        handles,
        descriptor,
        &image,
    );
    let pending = base
        .plan()
        .schema_validation()
        .bindings()
        .prepare_runtime_scene_replacement_with_claims(
            world,
            scene.candidate(),
            current,
            assets,
            claims,
            SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
            &[],
            |writer| {
                writer.replace_resource(products);
            },
        )
        .unwrap_or_else(|report| panic!("TileSet replacement preparation failed: {report:?}"));
    (pending, handles)
}

fn assert_current(
    world: &World,
    current: &SpawnedSceneInstance,
    entity: Entity,
    handles: PairHandles,
    baseline: &ProductBaseline,
) {
    assert_eq!(resolve(world, current), entity);
    assert_eq!(
        world.get::<Tilemap>(entity).unwrap().tileset,
        Some(handles.tileset)
    );
    let products = world.resource::<TileProducts>();
    assert!(products.matches_baseline(baseline));
    let image = products.capture_image(world.resource::<AssetServer>(), handles.image);
    assert!(image.is_current(
        world.resource::<AssetServer>(),
        &products.states,
        &products.images
    ));
    assert_eq!(
        products
            .tilesets
            .get(handles.tileset)
            .unwrap()
            .image_content(),
        Some(&image)
    );
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
}

#[test]
fn real_import_reaches_source_free_primary_and_auxiliary_tilemap_publication_and_retirement() {
    let fixture = Fixture::new();
    let base = fixture.capture();
    let (mut world, current, primary) = initial_world(&base);
    let old = resolve(&world, &current);
    let primary_content = world
        .resource::<TileProducts>()
        .capture_image(world.resource::<AssetServer>(), primary.image);
    assert_eq!(primary_content.pixels(), fixture.primary_pixels);
    assert_eq!(primary_content.extent(), fixture.extent);
    assert_eq!(primary_content.color_space(), ImageColorSpace::Srgb);
    let baseline = world.resource::<TileProducts>().capture_baseline();
    let server = world.resource::<AssetServer>().clone();
    let (pending, auxiliary) = prepare_next(
        &base,
        &mut world,
        &current,
        primary,
        &SceneNativeRetirementClaims::default(),
    );
    assert_current(&world, &current, old, primary, &baseline);
    assert_eq!(world.resource::<AssetServer>(), &server);
    assert!(
        world
            .resource::<TileProducts>()
            .images
            .get(auxiliary.image)
            .is_none()
    );
    assert!(
        world
            .resource::<TileProducts>()
            .tilesets
            .get(auxiliary.tileset)
            .is_none()
    );
    let report = pending
        .commit(&mut world)
        .unwrap_or_else(|_| panic!("matching TileSet commit failed"));
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    let replacement = report.instance.unwrap();
    assert!(world.get_entity(old).is_err());
    let tilemap = world.get::<Tilemap>(resolve(&world, &replacement)).unwrap();
    assert_eq!(tilemap.tileset, Some(auxiliary.tileset));
    assert_eq!(
        tilemap.get_cell(TileCoord::new(2, -3)).unwrap().tile,
        TileIndex::new(1)
    );
    let products = world.resource::<TileProducts>();
    assert!(!products.matches_baseline(&baseline));
    assert!(products.images.get(primary.image).is_none());
    assert!(products.tilesets.get(primary.tileset).is_none());
    let tileset = products.tilesets.get(auxiliary.tileset).unwrap();
    let descriptor = acquire_descriptor(&base, true);
    assert_eq!(tileset.atlas(), Some(descriptor.atlas()));
    assert_eq!(tileset.material().sampler, descriptor.sampler());
    assert_eq!(tileset.material().alpha_mode, descriptor.alpha_mode());
    assert_eq!(tileset.material().tint, descriptor.tint());
    let content = tileset.image_content().unwrap();
    assert_eq!(content.product(), image_product(true));
    assert_eq!(content.pixels(), fixture.auxiliary_pixels);
    assert_eq!(content.color_space(), ImageColorSpace::Linear);
    assert!(content.source().is_none());
    assert!(content.is_current(
        world.resource::<AssetServer>(),
        &products.states,
        &products.images
    ));
    assert!(!primary_content.is_authorized(
        world.resource::<AssetServer>(),
        &products.states,
        &products.images
    ));
    assert_eq!(primary_content.pixels(), fixture.primary_pixels);
    for (id, reference) in [
        (auxiliary.tileset.id(), tileset_product(true)),
        (auxiliary.image.id(), image_product(true)),
    ] {
        assert_eq!(world.resource::<AssetServer>().product(id), Some(reference));
        assert!(world.resource::<AssetServer>().path(id).is_none());
        let state = products.states.state(id).unwrap();
        assert!(state.source_hash().is_none());
        assert!(state.import_hash().is_none());
    }
    let exported = base
        .plan()
        .schema_validation()
        .bindings()
        .export_scene(&world)
        .into_output()
        .unwrap();
    let value = &exported.document.entities[0].components[&ComponentTypeId::new(TILEMAP)].value;
    assert_eq!(
        decode_asset_reference_value(value.get("tileset").unwrap()).unwrap(),
        AssetRef::Product(tileset_product(true))
    );
    let receipt = products.receipt(auxiliary.tileset);
    retire_and_despawn_scene_instance(&mut world, &replacement).unwrap();
    let mut products = world.remove_resource::<TileProducts>().unwrap();
    let mut server = world.remove_resource::<AssetServer>().unwrap();
    products.retire(&server, &[receipt]).unwrap();
    products.remove_image(auxiliary.image);
    products.reconcile(&mut server);
    assert!(products.owner.is_empty());
    assert!(products.tilesets.get(auxiliary.tileset).is_none());
    assert!(products.images.get(auxiliary.image).is_none());
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
}

#[test]
fn actual_descriptor_image_dependency_rejects_omitted_soft_wrong_and_absent_products() {
    for case in [
        FixtureCase::OmittedDependency,
        FixtureCase::SoftDependency,
        FixtureCase::WrongImageDependency,
        FixtureCase::MissingImageMember,
    ] {
        let fixture = Fixture::with_case(case);
        let base = fixture.capture();
        let (world, current, primary) = initial_world(&base);
        let old = resolve(&world, &current);
        let baseline = world.resource::<TileProducts>().capture_baseline();
        let server = world.resource::<AssetServer>().clone();
        // The Scene edge itself is correct. Only the decoded TileSet reveals its Image demand.
        assert!(base.acquire_scene(product(LATER_SOURCE, false)).is_ok());
        let result =
            base.acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(tileset_product(true));
        if matches!(case, FixtureCase::MissingImageMember) {
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
        assert_current(&world, &current, old, primary, &baseline);
        assert_eq!(world.resource::<AssetServer>(), &server);
    }
}

#[test]
fn wrong_image_family_and_receiving_type_claim_reject_before_publication() {
    let fixture = Fixture::with_case(FixtureCase::WrongImageFamily);
    let base = fixture.capture();
    let (world, current, primary) = initial_world(&base);
    let old = resolve(&world, &current);
    let baseline = world.resource::<TileProducts>().capture_baseline();
    let descriptor = acquire_descriptor(&base, true);
    assert!(matches!(
        base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(descriptor.image_product()),
        Err(RuntimeContentError::CodecMismatch)
    ));
    // The member really is another admitted family, rather than corrupted Image bytes.
    assert!(
        base.acquire_asset::<TileSetDomain, TileSetRuntimeCodec>(descriptor.image_product())
            .is_ok()
    );
    let mut bindings = SceneAssetBindings::from_world(&world);
    bindings
        .reserve_product::<TileSet>(descriptor.image_product())
        .unwrap();
    assert!(matches!(
        bindings.reserve_product::<ImageAsset>(descriptor.image_product()),
        Err(AssetError::AssetTypeMismatch { .. })
    ));
    drop(bindings);
    assert_current(&world, &current, old, primary, &baseline);
}

#[test]
fn actual_image_extent_and_persisted_atlas_membership_reject_without_live_mutation() {
    for case in [FixtureCase::WrongImageExtent, FixtureCase::OutsideAtlas] {
        let fixture = Fixture::with_case(case);
        let base = fixture.capture();
        let (world, current, primary) = initial_world(&base);
        let old = resolve(&world, &current);
        let baseline = world.resource::<TileProducts>().capture_baseline();
        let server = world.resource::<AssetServer>().clone();
        let scene = base.acquire_scene(product(LATER_SOURCE, false)).unwrap();
        let descriptor = acquire_descriptor(&base, true);
        let image = acquire_image(&base, true);
        let mut bindings = SceneAssetBindings::from_world(&world);
        let handles = PairHandles::reserve(&mut bindings, descriptor);
        if matches!(case, FixtureCase::WrongImageExtent) {
            validate_cells(scene.candidate(), descriptor).unwrap();
            let mut products = TileProducts::default();
            assert!(matches!(
                products.publish(
                    bindings.staged_server(),
                    handles,
                    descriptor,
                    retained_image(&image),
                    None,
                    &[],
                ),
                Err(TileSetAssetError::ImageExtentMismatch)
            ));
            assert!(products.images.is_empty());
            assert!(products.tilesets.is_empty());
            assert!(products.states.is_empty());
            assert!(products.events.is_empty());
            assert!(products.owner.is_empty());
        } else {
            assert!(matches!(
                validate_cells(scene.candidate(), descriptor),
                Err(TilemapMembershipError::TileOutsideAtlas { tileset, coord, tile })
                    if tileset == AssetRef::Product(descriptor.product())
                        && coord == TileCoord::new(2, -3)
                        && tile == TileIndex::new(fixture.extent.width * fixture.extent.height)
            ));
        }
        assert_current(&world, &current, old, primary, &baseline);
        assert_eq!(world.resource::<AssetServer>(), &server);
    }
}

#[test]
fn cancellation_and_changed_asset_bindings_keep_the_current_tileset_and_scene() {
    for stale in [false, true] {
        let fixture = Fixture::new();
        let base = fixture.capture();
        let (mut world, current, primary) = initial_world(&base);
        let old = resolve(&world, &current);
        let baseline = world.resource::<TileProducts>().capture_baseline();
        let identities = world.resource::<WorldIdentityDomain>().stats();
        let (pending, next) = prepare_next(
            &base,
            &mut world,
            &current,
            primary,
            &SceneNativeRetirementClaims::default(),
        );
        assert_current(&world, &current, old, primary, &baseline);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
        let pending = if stale {
            let bindings = SceneAssetBindings::from_world(&world);
            world
                .resource_mut::<AssetServer>()
                .reserve_product::<String>(product(UNRELATED_SOURCE, false))
                .unwrap();
            assert!(matches!(
                bindings.validate(&world),
                Err(ComponentCodecError::AssetServerChanged)
            ));
            let server = world.resource::<AssetServer>().clone();
            let (pending, report) = pending
                .commit(&mut world)
                .err()
                .unwrap_or_else(|| panic!("stale bindings committed"));
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "scene.prepared-baseline-changed")
            );
            assert_eq!(world.resource::<AssetServer>(), &server);
            pending
        } else {
            pending
        };
        pending
            .discard(&mut world)
            .unwrap_or_else(|_| panic!("TileSet candidate discard failed"));
        assert_current(&world, &current, old, primary, &baseline);
        assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
        assert!(
            world
                .resource::<AssetServer>()
                .get_handle::<TileSet>(next.tileset.id())
                .is_none()
        );
        assert!(
            world
                .resource::<AssetServer>()
                .get_handle::<ImageAsset>(next.image.id())
                .is_none()
        );
        assert!(
            world
                .resource::<TileProducts>()
                .tilesets
                .get(next.tileset)
                .is_none()
        );
        assert!(
            world
                .resource::<TileProducts>()
                .images
                .get(next.image)
                .is_none()
        );
        retire_and_despawn_scene_instance(&mut world, &current).unwrap();
    }
}

#[derive(Component)]
struct NativeTileCapture;

#[test]
fn native_claim_failure_keeps_actual_resources_and_retries_the_same_complete_scene_transaction() {
    let fixture = Fixture::new();
    let base = fixture.capture();
    let (mut world, current, primary) = initial_world(&base);
    let old = resolve(&world, &current);
    world.entity_mut(old).insert(NativeTileCapture);
    let claims = SceneNativeRetirementClaims::from_component_ids([world
        .component_id::<NativeTileCapture>()
        .unwrap()]);
    let baseline = world.resource::<TileProducts>().capture_baseline();
    let server = world.resource::<AssetServer>().clone();
    let identities = world.resource::<WorldIdentityDomain>().stats();
    let (pending, next) = prepare_next(&base, &mut world, &current, primary, &claims);
    pending.validate_baseline(&mut world).unwrap();
    let (pending, report) = pending
        .commit(&mut world)
        .err()
        .unwrap_or_else(|| panic!("unretired native claim committed"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.native-retirement-incomplete")
    );
    assert_current(&world, &current, old, primary, &baseline);
    assert_eq!(world.resource::<AssetServer>(), &server);
    assert_eq!(world.resource::<WorldIdentityDomain>().stats(), identities);
    assert!(
        world
            .resource::<TileProducts>()
            .tilesets
            .get(next.tileset)
            .is_none()
    );
    world.entity_mut(old).remove::<NativeTileCapture>();
    let report = pending
        .commit(&mut world)
        .unwrap_or_else(|_| panic!("retired native claim did not retry"));
    let replacement = report.instance.unwrap();
    assert!(!report.diagnostics.has_errors());
    assert!(world.get_entity(old).is_err());
    assert_eq!(
        world
            .get::<Tilemap>(resolve(&world, &replacement))
            .unwrap()
            .tileset,
        Some(next.tileset)
    );
    let products = world.resource::<TileProducts>();
    let image = products.capture_image(world.resource::<AssetServer>(), next.image);
    assert_eq!(
        products.tilesets.get(next.tileset).unwrap().image_content(),
        Some(&image)
    );
    assert_eq!(image.pixels(), fixture.auxiliary_pixels);
    assert_eq!(
        world.resource::<AssetServer>().product(next.tileset.id()),
        Some(tileset_product(true))
    );
    assert_eq!(
        world.resource::<AssetServer>().product(next.image.id()),
        Some(image_product(true))
    );
    assert!(products.tilesets.get(primary.tileset).is_none());
    assert!(products.images.get(primary.image).is_none());
    retire_and_despawn_scene_instance(&mut world, &replacement).unwrap();
}

#[test]
fn exact_aggregate_baseline_rejects_same_value_republication_and_resource_reinstallation() {
    let fixture = Fixture::new();
    let base = fixture.capture();
    let (mut world, current, primary) = initial_world(&base);
    let old = resolve(&world, &current);
    let (pending, next) = prepare_next(
        &base,
        &mut world,
        &current,
        primary,
        &SceneNativeRetirementClaims::default(),
    );
    let baseline = world.resource::<TileProducts>().capture_baseline();
    let server = world.resource::<AssetServer>().clone();
    // Mutate while detached from ECS: an ECS tick cannot observe this managed publication.
    let mut products = world.remove_resource::<TileProducts>().unwrap();
    let image = products
        .images
        .get(primary.image)
        .unwrap()
        .share_retained()
        .unwrap();
    let version = products.states.version(primary.image.id());
    products.republish_image(primary.image, image);
    assert_ne!(products.states.version(primary.image.id()), version);
    assert!(!products.matches_baseline(&baseline));
    world.insert_resource(products);
    let (pending, report) = pending
        .commit(&mut world)
        .err()
        .unwrap_or_else(|| panic!("superseded aggregate committed"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.product-resource-changed")
    );
    assert_eq!(world.resource::<AssetServer>(), &server);
    assert_eq!(resolve(&world, &current), old);
    assert!(
        world
            .resource::<TileProducts>()
            .tilesets
            .get(next.tileset)
            .is_none()
    );
    pending
        .discard(&mut world)
        .unwrap_or_else(|_| panic!("superseded candidate discard failed"));

    let (pending, next) = prepare_next(
        &base,
        &mut world,
        &current,
        primary,
        &SceneNativeRetirementClaims::default(),
    );
    let baseline = world.resource::<TileProducts>().capture_baseline();
    let original = world.resource::<TileProducts>();
    let mut replacement = TileProducts::default();
    let descriptor = acquire_descriptor(&base, false);
    replacement
        .publish(
            world.resource::<AssetServer>(),
            primary,
            descriptor,
            original
                .images
                .get(primary.image)
                .unwrap()
                .share_retained()
                .unwrap(),
            None,
            &[],
        )
        .unwrap();
    assert!(
        !replacement.matches_baseline(&baseline),
        "equal content in another store is a different incarnation"
    );
    world.insert_resource(replacement);
    let installed = world.resource::<TileProducts>().capture_baseline();
    let (pending, report) = pending
        .commit(&mut world)
        .err()
        .unwrap_or_else(|| panic!("reinstalled aggregate accepted an old baseline"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == "scene.product-resource-changed")
    );
    assert_current(&world, &current, old, primary, &installed);
    assert_eq!(world.resource::<AssetServer>(), &server);
    assert!(
        world
            .resource::<TileProducts>()
            .tilesets
            .get(next.tileset)
            .is_none()
    );
    pending
        .discard(&mut world)
        .unwrap_or_else(|_| panic!("reinstalled candidate discard failed"));
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
}

#[test]
fn descriptor_layout_barrier_and_superseded_receipts_use_the_actual_publication_owner() {
    let fixture = Fixture::new();
    let base = fixture.capture();
    let (mut world, current, handles) = initial_world(&base);
    retire_and_despawn_scene_instance(&mut world, &current).unwrap();
    let server = world.remove_resource::<AssetServer>().unwrap();
    let mut products = world.remove_resource::<TileProducts>().unwrap();
    let descriptor = acquire_descriptor(&base, false);
    let original = products.receipt(handles.tileset);
    let image = products
        .images
        .get(handles.image)
        .unwrap()
        .share_retained()
        .unwrap();
    let changed = descriptor.with_tint(Color::rgb(0.5, 0.25, 0.75)).unwrap();
    let current = products
        .publish(
            &server,
            handles,
            changed,
            image,
            Some(original.clone()),
            &[],
        )
        .unwrap();
    let baseline = products.capture_baseline();
    assert!(matches!(
        products.retire(&server, &[original]),
        Err(TileSetAssetError::Superseded)
    ));
    assert!(products.matches_baseline(&baseline));
    let changed_layout = TileSetDescriptor::new(
        descriptor.product(),
        descriptor.image_product(),
        TileAtlasLayout::grid(
            Vec2::new(fixture.extent.width as f32, fixture.extent.height as f32),
            1,
            1,
        ),
    )
    .unwrap();
    for overlap in [false, true] {
        let image = products
            .images
            .get(handles.image)
            .unwrap()
            .share_retained()
            .unwrap();
        let retirements = if overlap {
            vec![current.clone()]
        } else {
            vec![]
        };
        let result = products.publish(
            &server,
            handles,
            changed_layout,
            image,
            Some(current.clone()),
            &retirements,
        );
        if overlap {
            assert!(matches!(
                result,
                Err(TileSetAssetError::InvalidPublicationBatch)
            ));
        } else {
            assert!(matches!(
                result,
                Err(TileSetAssetError::AtlasLayoutChangeRequiresRebuild)
            ));
        }
        assert!(products.matches_baseline(&baseline));
    }
    products
        .retire(&server, std::slice::from_ref(&current))
        .unwrap();
    let image = products
        .images
        .get(handles.image)
        .unwrap()
        .share_retained()
        .unwrap();
    let rebuilt = products
        .publish(&server, handles, changed_layout, image, None, &[])
        .unwrap();
    assert_eq!(
        products.tilesets.get(handles.tileset).unwrap().atlas(),
        Some(changed_layout.atlas())
    );
    assert!(matches!(
        products.retire(&server, &[current]),
        Err(TileSetAssetError::Superseded)
    ));
    products.retire(&server, &[rebuilt]).unwrap();
    assert!(products.owner.is_empty());
}

#[test]
fn escaped_image_and_tileset_prepared_captures_keep_original_runtime_output_and_base_charged() {
    let fixture = Fixture::new();
    let base = fixture.capture_for_one_image();
    let descriptor = acquire_descriptor(&base, false);
    let value = acquire_image(&base, false);
    let owner = Arc::downgrade(&value);
    let charged = value.retained_bytes();
    let pixels = value.product().pixels().as_ptr();
    let mut server = AssetServer::new();
    let handles = PairHandles {
        tileset: server
            .reserve_product::<TileSet>(descriptor.product())
            .unwrap(),
        image: server
            .reserve_product::<ImageAsset>(descriptor.image_product())
            .unwrap(),
    };
    let mut products = TileProducts::default();
    let receipt = products
        .publish(
            &server,
            handles,
            descriptor,
            retained_image(&value),
            None,
            &[],
        )
        .unwrap();
    let image = products.capture_image(&server, handles.image);
    let prepared_image = image.prepared_snapshot();
    let tile_content = products
        .tilesets
        .get(handles.tileset)
        .unwrap()
        .image_content()
        .unwrap()
        .clone();
    let prepared_tile = tile_content.prepared_snapshot();
    let prepared_tile_clone = prepared_tile.clone();
    for prepared in [&prepared_image, &prepared_tile, &prepared_tile_clone] {
        assert_eq!(prepared.resource().pixels().as_ptr(), pixels);
        assert!(prepared.resource().has_retention_owner());
        assert_eq!(prepared.resource().pixels(), fixture.primary_pixels);
    }
    let retry_base = base.clone();
    drop((base, value));
    products.retire(&server, &[receipt]).unwrap();
    products.remove_image(handles.image);
    assert!(!image.is_authorized(&server, &products.states, &products.images));
    drop((products, server, image, tile_content));
    {
        let held = owner
            .upgrade()
            .expect("escaped captures retain the original acquisition");
        assert_eq!(held.retained_bytes(), charged);
        assert_eq!(held.product().pixels().as_ptr(), pixels);
        assert!(std::ptr::eq(held.base().catalog(), retry_base.catalog()));
        assert!(std::ptr::eq(held.base().plan(), retry_base.plan()));
    }
    let assert_full = || {
        assert!(matches!(
            retry_base.acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(true)),
            Err(RuntimeContentError::Budget {
                kind: RuntimeContentBudgetKind::RetainedBytes,
                ..
            })
        ))
    };
    assert_full();
    drop(prepared_image);
    assert!(owner.upgrade().is_some());
    assert_full();
    drop(prepared_tile);
    assert_eq!(
        prepared_tile_clone.resource().pixels(),
        fixture.primary_pixels
    );
    assert!(owner.upgrade().is_some());
    assert_full();
    drop(prepared_tile_clone);
    assert!(owner.upgrade().is_none());
    let recovered = retry_base
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(image_product(true))
        .unwrap();
    assert_eq!(recovered.product().pixels(), fixture.auxiliary_pixels);
    assert!(recovered.product().source().is_none());
}
