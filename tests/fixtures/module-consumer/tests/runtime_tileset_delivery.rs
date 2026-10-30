//! Independent processed TileSet delivery through the public Runtime and Scene owners.
//!
//! The aggregate tests own their Image/TileSet stores. The first-party HeadlessRun test separately
//! exercises stock native residency, extraction, ImagePrepare and Scene travel. It requires the
//! root presentation-2d feature, without a window or GPU backend. Neither path qualifies GPU upload
//! or a packaged Player.

use std::{
    fs,
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};

use nara_asset::{
    AssetError, AssetEvents, AssetProductRef, AssetRecord, AssetRef, AssetRuntimeError,
    AssetRuntimeLimits, AssetServer, AssetSlotRevision, AssetSourceKind, AssetSourcePath,
    AssetStateRevision, AssetStates, Assets, Handle, ImportedProductId, ProjectAssetDatabase,
    RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, Color, FormatKind, FormatVersion, ItemLimit, Vec2};
use nara_ecs::{Component, Entity, Resource, World};
use nara_engine::{
    ProductRecipe,
    advanced_prelude::{CurrentScene, commit_prepared_project_scene},
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, ProjectDomainAssets,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentAsset,
        RuntimeContentBase, RuntimeContentBudgetKind, RuntimeContentCatalog,
        RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentError, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
    sprite_render::{
        __private::{ExtractedSpriteKind, ExtractedSprites},
        SpriteRenderStats,
    },
};
use nara_fs::ContentDigest;
use nara_identity::{EntityLookup, SpawnedSceneInstance, WorldIdentityDomain};
use nara_image::{
    ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageContentSnapshot, ImageDomain,
    ImageExtent, ImageFormat, ImageImportSettings, ImageImporter, ImageRuntimeCodec,
    PreparedImageSnapshot, PreparedImages, image_resource_key,
};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_reflect::{
    ComponentCodecError, ComponentTypeId, ComponentValue, decode_asset_reference_value,
    encode_asset_reference_value,
};
use nara_scene::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneLimits,
    SceneComponentBindings, SceneComponentRecord, SceneEntityId, SceneOccurrenceKey,
    SourceDocumentKey, SourceDocumentKind,
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
// Keep the alias after both real sources so acquisition still selects the concrete Product first.
const NATIVE_ALIAS_SOURCE: &str = "fbb5dc12-516b-45e7-8e04-cad99a33f786";
const TILEMAP: &str = "nara.tilemap.Tilemap";
const SOURCE_FILE: &str = "authoring.png";
const PRIMARY_IMAGE_FILE: &str = "primary.runtime.image";
const AUXILIARY_IMAGE_FILE: &str = "auxiliary.runtime.image";
const PRIMARY_TILESET_FILE: &str = "primary.runtime.tileset";
const AUXILIARY_TILESET_FILE: &str = "auxiliary.runtime.tileset";
const STARTUP_FILE: &str = "startup.runtime.json";
const LATER_FILE: &str = "later.runtime.json";
const RETENTION_PROBE_FILE: &str = "retention.probe";
const RETENTION_PROBE_BYTES: &[u8] = b"runtime-native-retention-probe";
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
        .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.runtime-tileset.image",
            1,
        ))
        .unwrap()
        .add_runtime_tileset_codec::<TileSetRuntimeCodec>(RuntimeCodecDefinitionId::new(
            "consumer.runtime-tileset.tileset",
            1,
        ))
        .unwrap()
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
    presented: bool,
    direct_image: Option<AssetProductRef>,
) -> RuntimeSceneCandidate {
    let mut document = json!({
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
    });
    if presented {
        let transform = bindings
            .schema(&ComponentTypeId::new("nara.transform.Transform2d"))
            .unwrap();
        let record = SceneComponentRecord::new(
            transform.version(),
            transform.authoring_initial_value().unwrap().clone(),
        );
        document["payload"]["entities"][0]["components"]["nara.transform.Transform2d"] =
            serde_json::to_value(record).unwrap();
    }
    if let Some(image) = direct_image {
        let sprite = bindings
            .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
            .unwrap();
        let mut value = sprite.authoring_initial_value().unwrap().clone();
        let ComponentValue::Map(fields) = &mut value else {
            unreachable!("the Sprite initial value is an admitted map")
        };
        let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
            unreachable!("the Sprite initial value contains its material")
        };
        material.insert(
            "image".into(),
            encode_asset_reference_value(&AssetRef::Product(image)),
        );
        document["payload"]["entities"][0]["components"]["nara.sprite.Sprite"] =
            serde_json::to_value(SceneComponentRecord::new(sprite.version(), value)).unwrap();
    }
    let encoded = serde_json::to_vec(&document).unwrap();
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
    NativeOmittedDependency,
    NativeSoftDependency,
    NativeImageAlias,
    NativeTileSetAlias,
}

struct Fixture {
    directory: tempfile::TempDir,
    expected: RuntimeContentExpectation,
    recipe: ProductRecipe,
    primary_pixels: Vec<u8>,
    auxiliary_pixels: Vec<u8>,
    extent: ImageExtent,
}

impl Fixture {
    fn new() -> Self {
        Self::with_case(FixtureCase::Valid)
    }

    fn with_case(case: FixtureCase) -> Self {
        Self::with_recipe(case, recipe(), false)
    }

    fn with_recipe(case: FixtureCase, recipe: ProductRecipe, presented: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(SOURCE_FILE), SOURCE_PNG).unwrap();
        // This is the real producer import. Processed publication below never invents source facts.
        let imported = ImageImporter::new()
            .unwrap()
            .preflight_unpublished_import(ImageBytesImportRequest::new(
                AssetRecord::new(
                    stable(IMAGE_SOURCE),
                    AssetSourcePath::new(SOURCE_FILE).unwrap(),
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
        if presented {
            configuration
                .capabilities
                .requested
                .push(ProductCapability::Presentation2d);
        }
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
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
            let direct_image = (source == STARTUP_SOURCE
                && matches!(
                    case,
                    FixtureCase::NativeOmittedDependency | FixtureCase::NativeSoftDependency
                ))
            .then_some(image_product(false));
            let encoded = processed_scene(
                plan.schema_validation().bindings(),
                source,
                reference,
                tile,
                presented,
                direct_image,
            )
            .to_json_bytes()
            .unwrap();
            fs::write(directory.path().join(path), &encoded).unwrap();
            let mut dependencies = vec![RuntimeContentDependency::new(
                reference,
                RuntimeContentDependencyRole::Required,
            )];
            if let Some(image) = direct_image {
                dependencies.push(RuntimeContentDependency::new(
                    image,
                    RuntimeContentDependencyRole::Required,
                ));
            }
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Scene {
                        source: source_key(source),
                    },
                    vec![product(source, false)],
                    dependencies,
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
                match case {
                    FixtureCase::NativeOmittedDependency => vec![],
                    FixtureCase::NativeSoftDependency => vec![RuntimeContentDependency::new(
                        image_product(false),
                        RuntimeContentDependencyRole::Soft,
                    )],
                    _ => vec![RuntimeContentDependency::new(
                        image_product(false),
                        RuntimeContentDependencyRole::Required,
                    )],
                }
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
            let mut products = vec![descriptor.product()];
            if !auxiliary && matches!(case, FixtureCase::NativeTileSetAlias) {
                products.push(product(NATIVE_ALIAS_SOURCE, false));
            }
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(TileSetRuntimeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    products,
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
            let mut products = vec![image_product(auxiliary)];
            if !auxiliary && matches!(case, FixtureCase::NativeImageAlias) {
                products.push(product(NATIVE_ALIAS_SOURCE, false));
            }
            members.push(
                RuntimeContentMember::new(
                    path,
                    ContentDigest::of_bytes(&encoded),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(format).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    products,
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
        if presented {
            fs::write(
                directory.path().join(RETENTION_PROBE_FILE),
                RETENTION_PROBE_BYTES,
            )
            .unwrap();
            members.push(
                RuntimeContentMember::new(
                    RETENTION_PROBE_FILE,
                    ContentDigest::of_bytes(RETENTION_PROBE_BYTES),
                    RuntimeContentMemberKind::Asset {
                        format: FormatKind::new(RetentionProbeCodec::RUNTIME_KIND).unwrap(),
                        version: FormatVersion::ONE,
                    },
                    vec![product(UNRELATED_SOURCE, false)],
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
                runtime_codecs_fingerprint: plan.runtime_codecs_fingerprint().as_bytes(),
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
            recipe,
            primary_pixels,
            auxiliary_pixels,
            extent,
        }
    }

    fn open(&self, limits: RuntimeContentLimits) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.directory.path()).unwrap(),
            &self.expected,
            self.recipe.clone(),
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
            RETENTION_PROBE_FILE,
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

/// An unreferenced selected format whose output promise makes every probe refuse before decode.
/// Its Budget::required value observes the original base's live reservations without allocating.
struct RetentionProbeDomain;

impl nara_asset::AssetDomain for RetentionProbeDomain {
    type Product = ();
}

struct RetentionProbeCodec;

impl RuntimeAssetCodec<()> for RetentionProbeCodec {
    const RUNTIME_KIND: &'static str = "runtime_native_retention_probe";
    const RUNTIME_VERSION: u32 = 1;

    fn runtime_limits() -> AssetRuntimeLimits {
        AssetRuntimeLimits::new(
            ByteLimit::new(RETENTION_PROBE_BYTES.len()).unwrap(),
            ByteLimit::ONE,
            ByteLimit::new(2 * 1024 * 1024 * 1024).unwrap(),
        )
    }

    fn encode_runtime(_: &()) -> Result<Box<[u8]>, AssetRuntimeError> {
        Ok(RETENTION_PROBE_BYTES.into())
    }

    fn decode_runtime(_: &[u8]) -> Result<(), AssetRuntimeError> {
        panic!("the retention probe must refuse its output reservation before decode")
    }

    fn runtime_retained_bytes(_: &()) -> Result<usize, AssetRuntimeError> {
        Ok(0)
    }

    fn visit_required_runtime_products<E>(
        _: &(),
        _: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(_: &(), reference: AssetProductRef) -> bool {
        reference == product(UNRELATED_SOURCE, false)
    }
}

fn retained_requirement(base: &RuntimeContentBase) -> usize {
    match base.acquire_asset::<RetentionProbeDomain, RetentionProbeCodec>(product(
        UNRELATED_SOURCE,
        false,
    )) {
        Err(RuntimeContentError::Budget {
            kind: RuntimeContentBudgetKind::RetainedBytes,
            required,
            ..
        }) => required,
        result => panic!("expected a predecode retained-output refusal: {result:?}"),
    }
}

#[derive(Default, Clone, Resource)]
struct HeadlessTileOutcome {
    request: Option<ProjectSceneRequestId>,
    startup_entity: Option<Entity>,
    startup_handles: Option<PairHandles>,
    primary_image: Option<PreparedImageSnapshot>,
    primary_tileset_image: Option<PreparedImageSnapshot>,
    auxiliary_image: Option<PreparedImageSnapshot>,
    auxiliary_tileset_image: Option<PreparedImageSnapshot>,
    primary_presented: bool,
    auxiliary_presented: bool,
    inspected_prepared: bool,
    done: bool,
}

#[derive(Default)]
struct HeadlessTileConsumerPlugin;

impl Plugin for HeadlessTileConsumerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.runtime-tileset.headless"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            nara_image::IMAGE_PLUGIN_ID,
            nara_tilemap::TILESET_ASSET_PLUGIN_ID,
        ]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<HeadlessTileOutcome>()?;
        app.add_systems(CoreStage::Update, drive_headless_tile_travel)?;
        app.add_systems(CoreStage::Last, observe_headless_tile_frame)?;
        Ok(())
    }
}

fn assert_no_source_execution(world: &World) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<nara_asset::AssetSourceChanges>());
    assert!(!world.contains_resource::<nara_asset::AssetReloadRequests>());
    assert!(!world.contains_resource::<nara_asset::AssetReloadDiagnostics>());
    assert!(!world.contains_resource::<nara_asset::AssetLoadGenerations>());
    assert!(!world.contains_resource::<nara_asset::AssetDependencyGraph>());
    assert!(!world.contains_resource::<nara_image::ImageReloadStats>());
    assert!(!world.contains_resource::<TileProducts>());
}

/// Reads the stores used by stock extraction, without publishing through the aggregate consumer.
fn inspect_headless_tile(
    world: &World,
    auxiliary: bool,
) -> (
    Entity,
    PairHandles,
    PreparedImageSnapshot,
    PreparedImageSnapshot,
) {
    assert_no_source_execution(world);
    let current = CurrentScene::get(world).unwrap();
    let entity = resolve(world, current.receipt());
    let tilemap = world.get::<Tilemap>(entity).unwrap();
    assert_eq!(tilemap.cell_count(), 1);
    assert_eq!(
        tilemap.get_cell(TileCoord::new(2, -3)).unwrap().tile,
        TileIndex::new(u32::from(auxiliary))
    );
    let tileset_handle = tilemap.tileset.unwrap();
    let tilesets = world.resource::<Assets<TileSet>>();
    let tileset = tilesets.get(tileset_handle).unwrap();
    let image_handle = tileset.material().image.unwrap();
    let server = world.resource::<AssetServer>();
    assert_eq!(
        server.product(tileset_handle.id()),
        Some(tileset_product(auxiliary))
    );
    assert_eq!(
        server.product(image_handle.id()),
        Some(image_product(auxiliary))
    );
    assert!(server.path(tileset_handle.id()).is_none());
    assert!(server.path(image_handle.id()).is_none());
    let images = world.resource::<Assets<ImageAsset>>();
    let image = images.get(image_handle).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(tilesets.len(), 1);
    assert_eq!(world.resource::<TileSetAssets>().len(), 1);
    assert_eq!(image.product(), Some(image_product(auxiliary)));
    assert!(image.source().is_none());
    let states = world.resource::<AssetStates>();
    for id in [image_handle.id(), tileset_handle.id()] {
        let state = states.state(id).unwrap();
        assert!(state.usable_version().is_some());
        assert!(state.source_hash().is_none());
        assert!(state.import_hash().is_none());
    }
    let domains = world.resource::<ProjectDomainAssets>();
    let cpu_images = domains.collection::<ImageDomain>().unwrap();
    let cpu_tilesets = domains.collection::<TileSetDomain>().unwrap();
    assert_eq!(cpu_images.source_records().len(), 0);
    assert_eq!(cpu_tilesets.source_records().len(), 0);
    assert_eq!(cpu_images.iter().len(), 1);
    assert_eq!(cpu_tilesets.iter().len(), 1);
    let cpu_image = cpu_images
        .get(&AssetRef::Product(image_product(auxiliary)))
        .unwrap();
    assert_eq!(cpu_image.pixels().as_ptr(), image.pixels().as_ptr());
    let descriptor = cpu_tilesets
        .get(&AssetRef::Product(tileset_product(auxiliary)))
        .unwrap();
    assert_eq!(descriptor.product(), tileset_product(auxiliary));
    assert_eq!(descriptor.image_product(), image_product(auxiliary));
    assert_eq!(tileset.atlas(), Some(descriptor.atlas()));
    descriptor.validate_image_extent(image.extent()).unwrap();
    let image_content =
        ImageContentSnapshot::capture(image_handle, server, states, images).unwrap();
    let tile_content = tileset.image_content().unwrap();
    assert!(tile_content.is_current(server, states, images));
    assert!(tile_content.same_allocation(&image_content));
    let prepared_image = image_content.prepared_snapshot();
    let prepared_tile = tile_content.prepared_snapshot();
    for prepared in [&prepared_image, &prepared_tile] {
        assert!(prepared.resource().has_retention_owner());
        assert_eq!(
            prepared.resource().pixels().as_ptr(),
            image.pixels().as_ptr()
        );
    }
    (
        entity,
        PairHandles {
            image: image_handle,
            tileset: tileset_handle,
        },
        prepared_image,
        prepared_tile,
    )
}

fn drive_headless_tile_travel(world: &mut World) {
    assert_no_source_execution(world);
    if world.resource::<HeadlessTileOutcome>().done {
        return;
    }
    let Some(request) = world.resource::<HeadlessTileOutcome>().request else {
        if world
            .resource::<HeadlessTileOutcome>()
            .startup_entity
            .is_none()
        {
            let (entity, handles, image, tile) = inspect_headless_tile(world, false);
            let mut outcome = world.resource_mut::<HeadlessTileOutcome>();
            outcome.startup_entity = Some(entity);
            outcome.startup_handles = Some(handles);
            outcome.primary_image = Some(image);
            outcome.primary_tileset_image = Some(tile);
        } else if world.resource::<HeadlessTileOutcome>().primary_presented {
            let request = ProjectScenes::scope(world, |scenes| {
                scenes.request(product(LATER_SOURCE, false)).unwrap()
            });
            world.resource_mut::<HeadlessTileOutcome>().request = Some(request);
        }
        return;
    };
    let ready = ProjectScenes::scope(world, |scenes| {
        match scenes.state(request).unwrap() {
            ProjectSceneRequestState::Preparing => return false,
            ProjectSceneRequestState::Ready => {}
            state => panic!("unexpected first-party Scene request state: {state:?}"),
        }
        let view = scenes.prepared(request).unwrap();
        assert_eq!(view.selected_scene_path(), None);
        assert_eq!(
            view.selected_scene_product(),
            Some(product(LATER_SOURCE, false))
        );
        assert_eq!(view.scene().roots, [tilemap_id()]);
        let images = view.domain_assets().collection::<ImageDomain>().unwrap();
        let tilesets = view.domain_assets().collection::<TileSetDomain>().unwrap();
        assert_eq!(images.source_records().len(), 0);
        assert_eq!(tilesets.source_records().len(), 0);
        let image = images.get(&AssetRef::Product(image_product(true))).unwrap();
        assert_eq!(image.product(), Some(image_product(true)));
        assert!(image.source().is_none());
        assert!(
            images
                .get(&AssetRef::Product(image_product(false)))
                .is_none()
        );
        let descriptor = tilesets
            .get(&AssetRef::Product(tileset_product(true)))
            .unwrap();
        assert_eq!(descriptor.image_product(), image_product(true));
        descriptor.validate_image_extent(image.extent()).unwrap();
        true
    });
    if !ready {
        // Preparation runs on the existing I/O owner; yield without delaying the frame loop.
        std::thread::yield_now();
        return;
    }
    // Ready CPU data has not replaced the live native stores or current Scene.
    inspect_headless_tile(world, false);
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(ItemLimit::new(8).unwrap(), ItemLimit::ONE),
        &[],
        |_| {},
    );
    assert!(!report.diagnostics.has_errors(), "{report:?}");
    assert!(report.instance.is_some());
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed)
        );
        assert!(scenes.acknowledge(request));
    });
    let (entity, handles, image, tile) = inspect_headless_tile(world, true);
    let old = world
        .resource::<HeadlessTileOutcome>()
        .startup_entity
        .unwrap();
    let previous = world
        .resource::<HeadlessTileOutcome>()
        .startup_handles
        .unwrap();
    assert_ne!(entity, old);
    assert!(world.get_entity(old).is_err());
    assert_ne!(handles.image, previous.image);
    assert_ne!(handles.tileset, previous.tileset);
    assert!(
        world
            .resource::<Assets<ImageAsset>>()
            .get(previous.image)
            .is_none()
    );
    assert!(
        world
            .resource::<Assets<TileSet>>()
            .get(previous.tileset)
            .is_none()
    );
    let mut outcome = world.resource_mut::<HeadlessTileOutcome>();
    outcome.auxiliary_image = Some(image);
    outcome.auxiliary_tileset_image = Some(tile);
    outcome.inspected_prepared = true;
}

/// Observes actual stock extraction and ImagePrepare after all presentation stages have run.
fn observe_headless_tile_frame(world: &mut World) {
    let auxiliary = world.resource::<HeadlessTileOutcome>().inspected_prepared;
    let (entity, handles, _, _) = inspect_headless_tile(world, auxiliary);
    let stats = world.resource::<SpriteRenderStats>();
    assert_eq!(stats.extracted_sprites, 0);
    assert_eq!(stats.extracted_tile_cells, 1);
    assert_eq!(stats.missing_tilesets, 0);
    assert_eq!(stats.invalid_tile_regions, 0);
    assert_eq!(stats.unprepared_textures, 0);
    assert_eq!(stats.invalid_texture_regions, 0);
    let extracted = world.resource::<ExtractedSprites>().as_slice();
    assert_eq!(extracted.len(), 1);
    assert_eq!(extracted[0].entity, entity);
    assert_eq!(
        extracted[0].kind,
        ExtractedSpriteKind::TilemapCell {
            coord: TileCoord::new(2, -3),
            tile: TileIndex::new(u32::from(auxiliary)),
        }
    );
    assert_eq!(extracted[0].material.image, Some(handles.image));
    let content = extracted[0].image_content.as_ref().unwrap();
    assert_eq!(content.product(), image_product(auxiliary));
    assert!(content.is_current(
        world.resource::<AssetServer>(),
        world.resource::<AssetStates>(),
        world.resource::<Assets<ImageAsset>>(),
    ));
    let prepared_images = world.resource::<PreparedImages>();
    let prepared = prepared_images
        .get(image_resource_key(handles.image))
        .unwrap();
    assert_eq!(prepared_images.len(), 1);
    assert_eq!(
        prepared.resource().pixels().as_ptr(),
        content.pixels().as_ptr()
    );
    assert_eq!(prepared.resource().content_hash(), content.content_hash());
    assert!(prepared.resource().has_retention_owner());
    let prepared = prepared.clone();
    let mut outcome = world.resource_mut::<HeadlessTileOutcome>();
    if auxiliary {
        outcome.auxiliary_image = Some(prepared);
        outcome.auxiliary_presented = true;
        outcome.done = true;
    } else {
        outcome.primary_image = Some(prepared);
        outcome.primary_presented = true;
    }
}

#[test]
fn first_party_headless_run_publishes_and_travels_stock_tilesets_without_source_facets() {
    let recipe = recipe()
        .add_plugin::<HeadlessTileConsumerPlugin>()
        .unwrap()
        .add_runtime_asset_codec::<RetentionProbeDomain, RetentionProbeCodec>(
            RuntimeCodecDefinitionId::new("consumer.runtime-tileset.retention-probe", 1),
        )
        .unwrap();
    let fixture = Fixture::with_recipe(FixtureCase::Valid, recipe, true);
    let base = fixture.capture();
    for entry in base.plan().plugin_plan().entries() {
        assert_ne!(entry.plugin_id(), nara_asset::ASSET_SOURCE_PLUGIN_ID);
        assert_ne!(entry.plugin_id(), nara_image::IMAGE_SOURCE_PLUGIN_ID);
    }
    assert_eq!(fs::read_dir(fixture.directory.path()).unwrap().count(), 0);
    let baseline = retained_requirement(&base);
    let execution =
        HeadlessRunOptions::frames(NonZeroU32::new(4_000).unwrap(), Duration::from_millis(1))
            .stop_when(|outcome: &HeadlessTileOutcome| outcome.done);
    let mut run = HeadlessRun::from_runtime_content(base.clone(), execution, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut outcome = loop {
        let report = run.execute_bounded();
        assert!(report.startup_content_closure().is_none());
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => {
                assert!(
                    !report.diagnostics().has_errors(),
                    "{:?}",
                    report.diagnostics()
                );
                break outcome.clone();
            }
            HeadlessRunOutcome::Failed => {
                panic!(
                    "first-party processed TileSet run failed: {:?}",
                    report.diagnostics()
                );
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "first-party Runtime cleanup did not complete"
                );
                std::thread::yield_now();
            }
        }
    };
    drop(run);
    assert!(outcome.done);
    assert!(outcome.inspected_prepared);
    assert!(outcome.primary_presented);
    assert!(outcome.auxiliary_presented);
    let primary = outcome.primary_image.as_ref().unwrap();
    let auxiliary = outcome.auxiliary_image.as_ref().unwrap();
    assert_eq!(primary.resource().pixels(), fixture.primary_pixels);
    assert_eq!(primary.resource().extent(), fixture.extent);
    assert_eq!(auxiliary.resource().pixels(), fixture.auxiliary_pixels);
    assert_eq!(auxiliary.resource().extent(), fixture.extent);
    assert_eq!(primary.resource().color_space(), ImageColorSpace::Srgb);
    assert_eq!(auxiliary.resource().color_space(), ImageColorSpace::Linear);
    assert_eq!(
        primary.resource().pixels().as_ptr(),
        outcome
            .primary_tileset_image
            .as_ref()
            .unwrap()
            .resource()
            .pixels()
            .as_ptr()
    );
    assert_eq!(
        auxiliary.resource().pixels().as_ptr(),
        outcome
            .auxiliary_tileset_image
            .as_ref()
            .unwrap()
            .resource()
            .pixels()
            .as_ptr()
    );
    let escaped = outcome.primary_tileset_image.take().unwrap();
    let last = escaped.clone();
    drop(outcome);
    let held = retained_requirement(&base);
    assert!(
        held > baseline,
        "escaped native pixels retain the original output and metadata charge"
    );
    drop(escaped);
    assert_eq!(retained_requirement(&base), held);
    assert_eq!(last.resource().pixels(), fixture.primary_pixels);
    assert!(last.resource().has_retention_owner());
    drop(last);
    assert_eq!(retained_requirement(&base), baseline);
}

/// Deliberately trusts file-local identity; the concrete native owner must check full cardinality.
struct LaxImageIdentityCodec;

impl RuntimeAssetCodec<ImageAsset> for LaxImageIdentityCodec {
    const RUNTIME_KIND: &'static str = ImageRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = ImageRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        ImageRuntimeCodec::runtime_limits()
    }

    fn encode_runtime(value: &ImageAsset) -> Result<Box<[u8]>, AssetRuntimeError> {
        ImageRuntimeCodec::encode_runtime(value)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<ImageAsset, AssetRuntimeError> {
        ImageRuntimeCodec::decode_runtime(bytes)
    }

    fn runtime_retained_bytes(value: &ImageAsset) -> Result<usize, AssetRuntimeError> {
        ImageRuntimeCodec::runtime_retained_bytes(value)
    }

    fn visit_required_runtime_products<E>(
        value: &ImageAsset,
        visitor: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        ImageRuntimeCodec::visit_required_runtime_products(value, visitor)
    }

    fn contains_runtime_product(value: &ImageAsset, reference: AssetProductRef) -> bool {
        value
            .product()
            .is_some_and(|actual| actual.product() == reference.product())
    }
}

/// Omits the actual Image demand and trusts aliases to exercise the independent native checks.
struct LaxTileSetCodec;

impl RuntimeAssetCodec<TileSetDescriptor> for LaxTileSetCodec {
    const RUNTIME_KIND: &'static str = TileSetRuntimeCodec::RUNTIME_KIND;
    const RUNTIME_VERSION: u32 = TileSetRuntimeCodec::RUNTIME_VERSION;

    fn runtime_limits() -> AssetRuntimeLimits {
        TileSetRuntimeCodec::runtime_limits()
    }

    fn encode_runtime(value: &TileSetDescriptor) -> Result<Box<[u8]>, AssetRuntimeError> {
        TileSetRuntimeCodec::encode_runtime(value)
    }

    fn decode_runtime(bytes: &[u8]) -> Result<TileSetDescriptor, AssetRuntimeError> {
        TileSetRuntimeCodec::decode_runtime(bytes)
    }

    fn runtime_retained_bytes(value: &TileSetDescriptor) -> Result<usize, AssetRuntimeError> {
        TileSetRuntimeCodec::runtime_retained_bytes(value)
    }

    fn visit_required_runtime_products<E>(
        _: &TileSetDescriptor,
        _: impl FnMut(AssetProductRef) -> Result<(), E>,
    ) -> Result<(), E> {
        Ok(())
    }

    fn contains_runtime_product(value: &TileSetDescriptor, reference: AssetProductRef) -> bool {
        value.product().product() == reference.product()
    }
}

#[derive(Debug, Default, Clone, Resource)]
struct NativeAdmissionOutcome;

#[derive(Default)]
struct NativeAdmissionPlugin;

impl Plugin for NativeAdmissionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.runtime-tileset.native-admission"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<NativeAdmissionOutcome>()?;
        Ok(())
    }
}

fn lax_native_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_runtime_image_codec::<LaxImageIdentityCodec>(RuntimeCodecDefinitionId::new(
            "consumer.runtime-tileset.lax-image-identity",
            1,
        ))
        .unwrap()
        .add_runtime_tileset_codec::<LaxTileSetCodec>(RuntimeCodecDefinitionId::new(
            "consumer.runtime-tileset.lax-tileset",
            1,
        ))
        .unwrap()
        .add_plugin::<NativeAdmissionPlugin>()
        .unwrap()
}

fn assert_first_party_native_admission(base: RuntimeContentBase, refusal: Option<&str>) {
    let execution = HeadlessRunOptions::<NativeAdmissionOutcome>::frames(
        NonZeroU32::new(1).unwrap(),
        Duration::ZERO,
    );
    let mut run = HeadlessRun::from_runtime_content(base, execution, Vec::new());
    let mut report = run.execute_bounded();
    if refusal.is_none() {
        let deadline = Instant::now() + Duration::from_secs(15);
        while matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
            assert!(
                Instant::now() < deadline,
                "native admission control cleanup did not complete"
            );
            std::thread::yield_now();
            report = run.execute_bounded();
        }
    }
    assert!(report.startup_content_closure().is_none());
    if let Some(reason) = refusal {
        assert!(
            matches!(report.outcome(), HeadlessRunOutcome::Failed),
            "{report:?}"
        );
        let diagnostic = report
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.code().as_str() == "project.run.runtime-content-rejected")
            .expect(
                "the concrete native owner must refuse during content preparation before startup",
            );
        assert!(
            diagnostic.fields().iter().any(|field| {
                field.key().as_str() == "reason" && field.display_value() == reason
            }),
            "{diagnostic:?}"
        );
    } else {
        assert!(
            matches!(report.outcome(), HeadlessRunOutcome::Completed(_)),
            "{report:?}"
        );
        assert!(
            !report.diagnostics().has_errors(),
            "{:?}",
            report.diagnostics()
        );
    }
}

#[test]
fn first_party_native_tileset_requires_its_own_image_edge_even_with_empty_codec_inventory() {
    for case in [
        FixtureCase::NativeOmittedDependency,
        FixtureCase::NativeSoftDependency,
    ] {
        let fixture = Fixture::with_recipe(case, lax_native_recipe(), false);
        let base = fixture.capture();
        let startup = product(STARTUP_SOURCE, false);
        let scene = base.acquire_startup_scene().unwrap();
        let sprite = &scene.candidate().document().entities[0].components
            [&ComponentTypeId::new("nara.sprite.Sprite")];
        let image_leaf = sprite
            .value
            .field("material")
            .unwrap()
            .field("image")
            .unwrap();
        assert_eq!(
            decode_asset_reference_value(image_leaf).unwrap(),
            AssetRef::Product(image_product(false))
        );
        assert!(
            base.member(startup)
                .unwrap()
                .dependencies()
                .iter()
                .any(|dependency| {
                    dependency.product() == image_product(false)
                        && dependency.role() == RuntimeContentDependencyRole::Required
                })
        );
        assert!(
            base.member(tileset_product(false))
                .unwrap()
                .dependencies()
                .iter()
                .all(|dependency| {
                    dependency.product() != image_product(false)
                        || dependency.role() != RuntimeContentDependencyRole::Required
                })
        );
        // Both real values are generically decodable, and the Scene independently requires Image.
        let tileset = base
            .acquire_asset::<TileSetDomain, LaxTileSetCodec>(tileset_product(false))
            .unwrap();
        let image = base
            .acquire_asset::<ImageDomain, LaxImageIdentityCodec>(image_product(false))
            .unwrap();
        tileset
            .product()
            .validate_image_extent(image.product().extent())
            .unwrap();
        validate_cells(scene.candidate(), *tileset.product()).unwrap();
        drop((scene, tileset, image));
        assert_first_party_native_admission(base, Some("undeclared-dependency"));
    }
}

#[test]
fn first_party_native_cardinality_rejects_codec_approved_image_and_tileset_aliases() {
    for case in [
        FixtureCase::Valid,
        FixtureCase::NativeImageAlias,
        FixtureCase::NativeTileSetAlias,
    ] {
        let fixture = Fixture::with_recipe(case, lax_native_recipe(), false);
        let base = fixture.capture();
        let image = base
            .acquire_asset::<ImageDomain, LaxImageIdentityCodec>(image_product(false))
            .unwrap();
        let tileset = base
            .acquire_asset::<TileSetDomain, LaxTileSetCodec>(tileset_product(false))
            .unwrap();
        assert_eq!(image.product().product(), Some(image_product(false)));
        assert_eq!(tileset.product().product(), tileset_product(false));
        let actual = match case {
            FixtureCase::NativeImageAlias => Some(image_product(false)),
            FixtureCase::NativeTileSetAlias => Some(tileset_product(false)),
            _ => None,
        };
        if let Some(actual) = actual {
            let alias = product(NATIVE_ALIAS_SOURCE, false);
            assert_ne!(actual, alias);
            assert_eq!(actual.product(), alias.product());
            let member = base.member(actual).unwrap();
            assert_eq!(member.products(), &[actual, alias]);
            assert!(std::ptr::eq(base.member(alias).unwrap(), member));
        }
        drop((image, tileset));
        assert_first_party_native_admission(base, actual.map(|_| "product-identity-mismatch"));
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
