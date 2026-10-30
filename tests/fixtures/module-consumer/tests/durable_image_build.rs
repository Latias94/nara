//! Real PNG -> durable source-free Image -> processed CPU cook -> stock Runtime publication.

#![cfg(any(windows, target_os = "linux"))]

use std::{
    fs::{self, File},
    num::NonZeroU32,
    path::Path,
    sync::OnceLock,
    time::{Duration, Instant},
};

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaIdentity, AssetPath, AssetPathScope, AssetProductRef,
    AssetRef, AssetServer, AssetSourceKind, AssetSourcePath, AssetStates, Assets,
    ImportedProductId, ProjectAssetDatabase, RuntimeAssetCodec, StableAssetId,
};
use nara_core::{ByteLimit, FormatKind, FormatVersion};
use nara_ecs::{Resource, World};
use nara_engine::{
    ProductRecipe,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset_build::{
        ArtifactBuildDefinition, ArtifactBuildError, ArtifactBuildLimits, ArtifactBuildObservation,
        ArtifactBuildOutput, ArtifactBuildReason, ArtifactBuildResult, ArtifactBuildStatus,
        ArtifactBuildStore, ArtifactContentDigest, ArtifactGroupCandidate,
        ArtifactPublicationPoint, ArtifactSourceRequest, VerifiedArtifactGroup, build_image_import,
        cook_image_group,
    },
    project::{ProductCapability, RuntimeConfiguration, RuntimePreset},
    project_host::{
        HeadlessRun, HeadlessRunOptions, HeadlessRunOutcome, ProjectDomainAssets,
        open_explicit_product_project_root, resolve_runtime_configuration,
    },
    runtime_content::{
        RUNTIME_CONTENT_CATALOG_FILE, RuntimeCodecDefinitionId, RuntimeContentBase,
        RuntimeContentCatalog, RuntimeContentCatalogDefinition, RuntimeContentCatalogLimits,
        RuntimeContentDependency, RuntimeContentDependencyRole, RuntimeContentExpectation,
        RuntimeContentLimits, RuntimeContentMember, RuntimeContentMemberKind, RuntimeContentTarget,
    },
};
use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FileCapability, HostCapabilityOptions,
    RelativePath, TrustMode,
};
use nara_image::{
    ImageAsset, ImageColorSpace, ImageContentSnapshot, ImageCookError, ImageCookLimits,
    ImageCookSettings, ImageCookTarget, ImageDomain, ImageExtent, ImageImportSettings,
    ImageOversizePolicy, ImageRuntimeCodec, PreparedImageSnapshot,
};
use nara_reflect::{ComponentTypeId, ComponentValue, encode_asset_reference_value};
use nara_scene::{
    RuntimeSceneDocumentCandidate, RuntimeSceneLimits, SceneComponentRecord, SourceDocumentKey,
    SourceDocumentKind,
};

const SOURCE: &str = "d6210000-0000-4000-8000-000000000001";
const SCENE: &str = "d6210000-0000-4000-8000-000000000002";
const PNG_PATH: &str = "stripe.png";

fn product() -> AssetProductRef {
    primary(SOURCE)
}

fn primary(id: &str) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(id).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

/// The test Host explicitly authorizes this executable's bytes, once per test process. A revision
/// string alone is deliberately insufficient implementation evidence for either operation.
fn definition(id: &str) -> ArtifactBuildDefinition {
    static IMPLEMENTATION: OnceLock<ArtifactContentDigest> = OnceLock::new();
    ArtifactBuildDefinition {
        id: id.to_owned(),
        revision: FormatVersion::ONE,
        implementation: *IMPLEMENTATION.get_or_init(|| {
            let file = File::open(std::env::current_exe().unwrap()).unwrap();
            let authorized =
                FileCapability::from_host_handle(file, TrustMode::TrustedLocal, 0).unwrap();
            ArtifactContentDigest::from_content(authorized.digest(1024 * 1024 * 1024).unwrap())
        }),
    }
}

fn authority(path: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let handle = File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

struct Fixture {
    root: tempfile::TempDir,
    pixels: Vec<u8>,
    png: Vec<u8>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pixels = (0..4096u32)
            .flat_map(|x| {
                [
                    x as u8,
                    (x >> 8) as u8,
                    x.wrapping_mul(17).wrapping_add(3) as u8,
                    [0, 1, 127, 255][x as usize % 4],
                ]
            })
            .collect::<Vec<_>>();
        let png = rgba_png(&pixels);
        fs::write(root.path().join(PNG_PATH), &png).unwrap();
        let fixture = Self { root, pixels, png };
        fixture.settings(ImageColorSpace::Srgb);
        fixture
    }

    fn settings(&self, space: ImageColorSpace) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                product().source(),
                AssetPath::new(PNG_PATH).unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(space),
        )
        .to_json_string()
        .unwrap();
        fs::write(self.root.path().join(format!("{PNG_PATH}.meta")), metadata).unwrap();
    }

    fn request(&self) -> ArtifactSourceRequest {
        ArtifactSourceRequest::new(
            authority(self.root.path()),
            AssetPathScope::under(AssetSourcePath::new("assets").unwrap()),
            AssetPath::new(PNG_PATH).unwrap(),
            definition("consumer.durable-image.png-import"),
        )
    }

    fn store(&self, name: &str) -> ArtifactBuildStore {
        let path = self.root.path().join(name);
        fs::create_dir_all(&path).unwrap();
        ArtifactBuildStore::open(authority(&path), ArtifactBuildLimits::default()).unwrap()
    }

    fn import(&self, store: &ArtifactBuildStore) -> ArtifactBuildResult {
        build_image_import(store, &self.request(), |_| Ok(())).unwrap()
    }

    fn clear_cache(&self, name: &str) {
        let path = self.root.path().join(name).canonicalize().unwrap();
        assert!(path.starts_with(self.root.path().canonicalize().unwrap()));
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            fs::remove_file(entry.path()).unwrap();
        }
    }
}

fn limits(cap: u32) -> ImageCookLimits {
    let pixels = cap as usize * 4;
    let retained =
        size_of::<ImageAsset>() + size_of::<Box<[u8]>>() + 2 * size_of::<usize>() + pixels;
    ImageCookLimits::new(
        ByteLimit::new(retained).unwrap(),
        ByteLimit::new(retained + pixels).unwrap(),
    )
}

fn target(cap: u32) -> ImageCookTarget {
    ImageCookTarget::new(NonZeroU32::new(cap).unwrap())
}

fn fit() -> ImageCookSettings {
    ImageCookSettings {
        oversize: ImageOversizePolicy::FitNearest,
    }
}

fn cook(
    store: &ArtifactBuildStore,
    input: &VerifiedArtifactGroup,
    cap: u32,
) -> ArtifactBuildResult {
    cook_image_group(
        store,
        input,
        product(),
        definition("consumer.durable-image.cpu-cook"),
        fit(),
        target(cap),
        limits(cap),
        |_| Ok(()),
    )
    .unwrap()
}

fn decode(group: &VerifiedArtifactGroup) -> ImageAsset {
    assert_eq!(group.members().len(), 1);
    let member = &group.members()[0];
    assert_eq!(member.name, "runtime");
    assert_eq!(member.format.as_str(), ImageRuntimeCodec::RUNTIME_KIND);
    assert_eq!(member.format_version, FormatVersion::ONE);
    assert_eq!(member.products, [product()]);
    assert!(member.dependencies.is_empty());
    let bytes = group.member_bytes("runtime").unwrap();
    assert_eq!(member.digest, ArtifactContentDigest::of_bytes(bytes));
    let value = ImageRuntimeCodec::decode_runtime(bytes).unwrap();
    assert_eq!(value.product(), Some(product()));
    assert!(value.source().is_none());
    assert!(value.share_retained().is_none());
    value
}

#[test]
fn two_cold_stores_reproduce_import_and_cook_and_fresh_open_reuses_verified_members() {
    let fixture = Fixture::new();
    let left = fixture.store("left");
    let right = fixture.store("right");
    let a = fixture.import(&left);
    let b = fixture.import(&right);
    assert_eq!(a.status(), ArtifactBuildStatus::Built);
    assert_eq!(b.status(), ArtifactBuildStatus::Built);
    assert_eq!(a.group().digest(), b.group().digest());
    assert_eq!(a.group().recipe_digest(), b.group().recipe_digest());
    assert_eq!(decode(a.group()).pixels(), fixture.pixels);
    assert_eq!(
        a.group().seed().primary,
        Some(ArtifactContentDigest::of_bytes(&fixture.png))
    );
    assert_eq!(
        a.group().seed().location.as_ref().unwrap().as_str(),
        "assets/stripe.png"
    );
    assert_eq!(
        a.group().seed().definitions,
        [definition("consumer.durable-image.png-import")]
    );
    let normalized =
        AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(&a.group().seed().settings)
            .unwrap();
    assert_eq!(
        *normalized.settings(),
        ImageImportSettings::new(ImageColorSpace::Srgb)
    );
    let cooked_a = cook(&left, a.group(), 2048);
    let cooked_b = cook(&right, b.group(), 2048);
    assert_eq!(cooked_a.status(), ArtifactBuildStatus::Built);
    assert_eq!(cooked_a.group().digest(), cooked_b.group().digest());
    assert_eq!(
        cooked_a.group().member_bytes("runtime"),
        cooked_b.group().member_bytes("runtime")
    );
    drop(left);
    let reopened = fixture.store("left");
    let warm = fixture.import(&reopened);
    assert_eq!(warm.status(), ArtifactBuildStatus::Hit);
    assert_eq!(warm.reason(), &ArtifactBuildReason::Reused);
    let warm_cook = cook(&reopened, warm.group(), 2048);
    assert_eq!(warm_cook.status(), ArtifactBuildStatus::Hit);
    assert_eq!(warm_cook.group().digest(), cooked_a.group().digest());
}

#[test]
fn different_png_encoding_with_identical_runtime_bytes_invalidates_the_processed_observation() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let original = fixture.import(&store).into_group();
    let old_cook = cook(&store, &original, 2048).into_group();
    // Split one IDAT into two consecutive chunks without changing the actual zlib stream.
    let length = u32::from_be_bytes(fixture.png[33..37].try_into().unwrap()) as usize;
    assert_eq!(&fixture.png[37..41], b"IDAT");
    let zlib = &fixture.png[41..41 + length];
    let mut alternate = fixture.png[..33].to_vec();
    png_chunk(&mut alternate, b"IDAT", &zlib[..zlib.len() / 2]);
    png_chunk(&mut alternate, b"IDAT", &zlib[zlib.len() / 2..]);
    png_chunk(&mut alternate, b"IEND", &[]);
    fs::write(fixture.root.path().join(PNG_PATH), alternate).unwrap();
    let current = fixture.import(&store).into_group();
    assert_ne!(original.digest(), current.digest());
    assert_eq!(
        original.member_bytes("runtime"),
        current.member_bytes("runtime")
    );
    let rebuilt = cook(&store, &current, 2048);
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        rebuilt.reason(),
        &ArtifactBuildReason::ProcessedInputChanged(product())
    );
    assert_eq!(rebuilt.group().seed(), old_cook.seed());
    assert_ne!(rebuilt.group().recipe_digest(), old_cook.recipe_digest());
    assert_eq!(
        rebuilt.group().member_bytes("runtime"),
        old_cook.member_bytes("runtime")
    );
    assert_eq!(
        cook(&store, &current, 2048).status(),
        ArtifactBuildStatus::Hit
    );
}

#[test]
fn identical_png_with_changed_color_space_rebuilds_import_and_processed_cook() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let old = fixture.import(&store);
    let old_cook = cook(&store, old.group(), 2048);
    fixture.settings(ImageColorSpace::Linear);
    assert_eq!(
        fs::read(fixture.root.path().join(PNG_PATH)).unwrap(),
        fixture.png
    );
    let new = fixture.import(&store);
    let new_cook = cook(&store, new.group(), 2048);
    assert_eq!(new.status(), ArtifactBuildStatus::Built);
    assert_eq!(new_cook.status(), ArtifactBuildStatus::Built);
    assert_eq!(old.group().seed().primary, new.group().seed().primary);
    assert_ne!(old.group().digest(), new.group().digest());
    assert_ne!(old_cook.group().digest(), new_cook.group().digest());
    assert_ne!(
        old_cook.group().recipe_digest(),
        new_cook.group().recipe_digest()
    );
    assert_eq!(decode(new.group()).color_space(), ImageColorSpace::Linear);
    assert_eq!(
        decode(new_cook.group()).color_space(),
        ImageColorSpace::Linear
    );
    assert_eq!(
        decode(old_cook.group()).color_space(),
        ImageColorSpace::Srgb
    );
    assert_eq!(
        decode(new_cook.group()).pixels(),
        decode(old_cook.group()).pixels()
    );
    assert_eq!(
        new_cook.group().observations(),
        &[ArtifactBuildObservation::Processed {
            product: product(),
            group: new.group().digest(),
            member: new.group().members()[0].digest,
            format: new.group().members()[0].format.clone(),
            format_version: FormatVersion::ONE,
        }]
    );
    assert_eq!(
        new_cook.group().seed().primary,
        Some(new.group().members()[0].digest)
    );
    assert_eq!(
        new_cook.group().seed().definitions,
        [definition("consumer.durable-image.cpu-cook"),]
    );
    assert_eq!(
        cook(&store, new.group(), 2048).status(),
        ArtifactBuildStatus::Hit
    );
}

#[test]
fn the_same_cooker_can_process_its_own_output_and_reuse_the_exact_upstream_group() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let imported = fixture.import(&store).into_group();
    let large = cook(&store, &imported, 4096);
    assert_eq!(large.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        large.group().seed().definitions,
        [definition("consumer.durable-image.cpu-cook")]
    );
    assert_eq!(
        large.group().observations(),
        &[ArtifactBuildObservation::Processed {
            product: product(),
            group: imported.digest(),
            member: imported.members()[0].digest,
            format: imported.members()[0].format.clone(),
            format_version: imported.members()[0].format_version,
        }]
    );
    assert_eq!(
        large.group().member_bytes("runtime"),
        imported.member_bytes("runtime")
    );

    let small = cook(&store, large.group(), 2048);
    assert_eq!(small.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        small.group().seed().definitions,
        [definition("consumer.durable-image.cpu-cook")]
    );
    assert_eq!(
        small.group().seed().primary,
        Some(large.group().members()[0].digest)
    );
    assert_eq!(
        small.group().observations(),
        &[ArtifactBuildObservation::Processed {
            product: product(),
            group: large.group().digest(),
            member: large.group().members()[0].digest,
            format: large.group().members()[0].format.clone(),
            format_version: large.group().members()[0].format_version,
        }]
    );
    assert_ne!(large.group().digest(), imported.digest());
    let expected = fixture
        .pixels
        .chunks_exact(8)
        .flat_map(|pair| pair[4..].iter().copied())
        .collect::<Vec<_>>();
    let decoded = decode(small.group());
    assert_eq!(decoded.extent(), ImageExtent::new(2048, 1));
    assert_eq!(decoded.pixels(), expected);

    let reopened = fixture.store("cache");
    let warm = cook(&reopened, large.group(), 2048);
    assert_eq!(warm.status(), ArtifactBuildStatus::Hit);
    assert_eq!(warm.group().digest(), small.group().digest());
    assert_eq!(warm.group().observations(), small.group().observations());
}

#[test]
fn changed_upstream_implementation_with_identical_runtime_bytes_rebuilds_the_same_cook_seed() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let original = fixture.import(&store).into_group();
    let old_cook = cook(&store, &original, 2048).into_group();
    let mut seed = original.seed().clone();
    // Model a separately selected upstream implementation with the captured real Image output.
    seed.definitions[0].implementation =
        ArtifactContentDigest::of_bytes(b"replacement Image importer implementation");
    assert_eq!(seed.definitions[0].id, original.seed().definitions[0].id);
    assert_eq!(
        seed.definitions[0].revision,
        original.seed().definitions[0].revision
    );
    assert_ne!(
        seed.definitions[0].implementation,
        original.seed().definitions[0].implementation
    );
    let member = &original.members()[0];
    let current = store
        .commit(
            ArtifactGroupCandidate {
                seed,
                observations: original.observations().to_vec(),
                outputs: vec![ArtifactBuildOutput {
                    name: member.name.clone(),
                    format: member.format.clone(),
                    format_version: member.format_version,
                    products: member.products.clone(),
                    dependencies: member.dependencies.clone(),
                    bytes: original.member_bytes("runtime").unwrap().into(),
                }],
            },
            |_| Ok(()),
        )
        .unwrap()
        .into_group();
    assert_ne!(current.digest(), original.digest());
    assert_ne!(current.recipe_digest(), original.recipe_digest());
    assert_eq!(current.members(), original.members());
    assert_eq!(
        current.member_bytes("runtime"),
        original.member_bytes("runtime")
    );

    let rebuilt = cook(&store, &current, 2048);
    assert_eq!(rebuilt.status(), ArtifactBuildStatus::Built);
    assert_eq!(
        rebuilt.reason(),
        &ArtifactBuildReason::ProcessedInputChanged(product())
    );
    assert_eq!(rebuilt.group().seed(), old_cook.seed());
    assert_ne!(rebuilt.group().recipe_digest(), old_cook.recipe_digest());
    assert_ne!(rebuilt.group().digest(), old_cook.digest());
    assert_eq!(
        rebuilt.group().member_bytes("runtime"),
        old_cook.member_bytes("runtime")
    );
    assert_eq!(
        rebuilt.group().observations(),
        &[ArtifactBuildObservation::Processed {
            product: product(),
            group: current.digest(),
            member: current.members()[0].digest,
            format: current.members()[0].format.clone(),
            format_version: current.members()[0].format_version,
        }]
    );
    let warm = cook(&store, &current, 2048);
    assert_eq!(warm.status(), ArtifactBuildStatus::Hit);
    assert_eq!(warm.group().digest(), rebuilt.group().digest());
}

#[test]
fn targets_transform_exact_pixels_and_equal_outputs_still_keep_distinct_recipes() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let imported = fixture.import(&store);
    let large = cook(&store, imported.group(), 4096);
    let small = cook(&store, imported.group(), 2048);
    assert_eq!(decode(large.group()).extent(), ImageExtent::new(4096, 1));
    assert_eq!(decode(large.group()).pixels(), fixture.pixels);
    let expected = fixture
        .pixels
        .chunks_exact(8)
        .flat_map(|pair| pair[4..].iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(decode(small.group()).extent(), ImageExtent::new(2048, 1));
    assert_eq!(decode(small.group()).pixels(), expected);
    assert_ne!(
        large.group().members()[0].digest,
        small.group().members()[0].digest
    );
    assert_eq!(
        cook(&store, imported.group(), 2048).status(),
        ArtifactBuildStatus::Hit
    );
    assert!(matches!(
        cook_image_group(
            &store,
            imported.group(),
            product(),
            definition("consumer.durable-image.cpu-cook"),
            ImageCookSettings::default(),
            target(2048),
            limits(4096),
            |_| Ok(()),
        ),
        Err(ArtifactBuildError::Cook(
            ImageCookError::TargetDimensionExceeded { .. }
        ))
    ));
    let larger = cook(&store, imported.group(), 8192);
    let reject = cook_image_group(
        &store,
        imported.group(),
        product(),
        definition("consumer.durable-image.cpu-cook"),
        ImageCookSettings::default(),
        target(4096),
        limits(4096),
        |_| Ok(()),
    )
    .unwrap();
    for equal in [larger.group(), reject.group()] {
        assert_eq!(
            equal.member_bytes("runtime"),
            large.group().member_bytes("runtime")
        );
        assert_ne!(equal.recipe_digest(), large.group().recipe_digest());
        assert_ne!(equal.digest(), large.group().digest());
    }
    assert!(matches!(
        cook_image_group(
            &store,
            imported.group(),
            product(),
            definition("consumer.durable-image.cpu-cook"),
            fit(),
            target(2048),
            ImageCookLimits::new(ByteLimit::ONE, limits(2048).max_allocation_bytes()),
            |_| Ok(()),
        ),
        Err(ArtifactBuildError::Cook(
            ImageCookError::OutputBytesExceeded { .. }
        ))
    ));
    assert!(matches!(
        cook_image_group(
            &store,
            imported.group(),
            product(),
            definition("consumer.durable-image.cpu-cook"),
            fit(),
            target(2048),
            ImageCookLimits::new(limits(2048).max_output_bytes(), ByteLimit::ONE),
            |_| Ok(()),
        ),
        Err(ArtifactBuildError::Cook(
            ImageCookError::AllocationBytesExceeded { .. }
        ))
    ));
}

#[test]
fn missing_or_corrupt_generated_members_are_invalid_cache_and_never_current_hits() {
    for corrupt in [false, true] {
        let fixture = Fixture::new();
        let store = fixture.store("cache");
        let original = fixture.import(&store).into_group();
        drop(store);
        let blob = fs::read_dir(fixture.root.path().join("cache"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.file_name().to_string_lossy().starts_with("blob-"))
            .unwrap();
        if corrupt {
            let mut bytes = fs::read(blob.path()).unwrap();
            *bytes.last_mut().unwrap() ^= 1;
            fs::write(blob.path(), bytes).unwrap();
        } else {
            fs::remove_file(blob.path()).unwrap();
        }
        let reopened = fixture.store("cache");
        assert!(matches!(
            build_image_import(&reopened, &fixture.request(), |_| Ok(())),
            Err(ArtifactBuildError::InvalidCache(_))
        ));
        assert_eq!(decode(&original).pixels(), fixture.pixels);
        // A verified group's captured member is independently usable even after its old blob is lost.
        let separate = fixture.store("cook-cache");
        assert_eq!(
            cook(&separate, &original, 2048).status(),
            ArtifactBuildStatus::Built
        );
    }
}

#[test]
fn metadata_is_admitted_before_a_warm_probe_and_observer_edits_are_revalidated_after_control() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let old = fixture.import(&store).into_group();
    fs::write(fixture.root.path().join(format!("{PNG_PATH}.meta")), b"{}").unwrap();
    assert!(matches!(
        build_image_import(&store, &fixture.request(), |_| Ok(())),
        Err(ArtifactBuildError::InvalidCandidate(_))
    ));
    fixture.settings(ImageColorSpace::Srgb);
    let result = build_image_import(&store, &fixture.request(), |point| {
        if point == ArtifactPublicationPoint::Reuse {
            fixture.settings(ImageColorSpace::Linear);
        }
        Ok(())
    });
    assert!(matches!(result, Err(ArtifactBuildError::Stale)));
    let result = build_image_import(&store, &fixture.request(), |point| {
        if point == ArtifactPublicationPoint::BeforeIndex {
            fixture.settings(ImageColorSpace::Srgb);
        }
        Ok(())
    });
    assert!(matches!(result, Err(ArtifactBuildError::Stale)));
    assert_eq!(fixture.import(&store).group().digest(), old.digest());
    fixture.settings(ImageColorSpace::Linear);
    let result = build_image_import(&store, &fixture.request(), |point| {
        if point == ArtifactPublicationPoint::AfterIndex {
            fixture.settings(ImageColorSpace::Srgb);
        }
        Ok(())
    });
    let Err(ArtifactBuildError::Uncertain { receipt, cause }) = result else {
        panic!("post-index source edit must preserve uncertain publication evidence")
    };
    assert!(matches!(*cause, ArtifactBuildError::Stale));
    assert_eq!(
        decode(receipt.group()).color_space(),
        ImageColorSpace::Linear
    );
}

#[test]
fn caller_selection_fence_applies_to_cold_cook_and_reuse_of_an_immutable_upstream() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let imported = fixture.import(&store).into_group();
    let cold = cook_image_group(
        &store,
        &imported,
        product(),
        definition("consumer.durable-image.cpu-cook"),
        fit(),
        target(2048),
        limits(2048),
        |point| {
            if point == ArtifactPublicationPoint::BeforeIndex {
                Err(ArtifactBuildError::Stale)
            } else {
                Ok(())
            }
        },
    );
    assert!(matches!(cold, Err(ArtifactBuildError::Stale)));
    assert_eq!(
        cook(&store, &imported, 2048).status(),
        ArtifactBuildStatus::Built
    );
    let warm = cook_image_group(
        &store,
        &imported,
        product(),
        definition("consumer.durable-image.cpu-cook"),
        fit(),
        target(2048),
        limits(2048),
        |point| {
            assert_eq!(point, ArtifactPublicationPoint::Reuse);
            Err(ArtifactBuildError::Stale)
        },
    );
    assert!(matches!(warm, Err(ArtifactBuildError::Stale)));
    let absent = AssetProductRef::new(
        product().source(),
        ImportedProductId::parse_subproduct("d6210000-0000-4000-8000-000000000003").unwrap(),
    )
    .unwrap();
    assert!(matches!(
        cook_image_group(
            &store,
            &imported,
            absent,
            definition("consumer.durable-image.cpu-cook"),
            fit(),
            target(2048),
            limits(2048),
            |_| Ok(()),
        ),
        Err(ArtifactBuildError::InvalidCandidate(_))
    ));
}

#[test]
fn two_actual_image_outputs_publish_as_one_group_and_interruption_keeps_the_complete_predecessor() {
    let fixture = Fixture::new();
    let producer = fixture.store("producer");
    let first = fixture.import(&producer).into_group();
    // The second source has independently authored identity, rather than an unused dummy blob.
    let other = primary("d6210000-0000-4000-8000-000000000004");
    let second_source = Fixture::new();
    let write_second = |space| {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                other.source(),
                AssetPath::new(PNG_PATH).unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(space),
        )
        .to_json_string()
        .unwrap();
        fs::write(
            second_source.root.path().join(format!("{PNG_PATH}.meta")),
            metadata,
        )
        .unwrap();
    };
    write_second(ImageColorSpace::Srgb);
    let second_store = second_source.store("producer");
    let second = second_source.import(&second_store).into_group();
    write_second(ImageColorSpace::Linear);
    let successor = second_source.import(&second_store).into_group();
    let candidate = |second: &VerifiedArtifactGroup| {
        let mut seed = first.seed().clone();
        seed.definitions = vec![definition("consumer.durable-image.pair")];
        let mut observations = Vec::new();
        let mut outputs = Vec::new();
        for (name, group, reference) in
            [("primary", &first, product()), ("secondary", second, other)]
        {
            let member = &group.members()[0];
            observations.push(ArtifactBuildObservation::Processed {
                product: reference,
                group: group.digest(),
                member: member.digest,
                format: member.format.clone(),
                format_version: member.format_version,
            });
            outputs.push(ArtifactBuildOutput {
                name: name.to_owned(),
                format: member.format.clone(),
                format_version: member.format_version,
                products: member.products.clone(),
                dependencies: member.dependencies.clone(),
                bytes: group.member_bytes("runtime").unwrap().into(),
            });
        }
        ArtifactGroupCandidate {
            seed,
            observations,
            outputs,
        }
    };
    let store = fixture.store("pair");
    let predecessor = store
        .commit(candidate(&second), |_| Ok(()))
        .unwrap()
        .into_group();
    for interruption in [
        ArtifactPublicationPoint::AfterBlob { index: 0 },
        ArtifactPublicationPoint::AfterBlob { index: 1 },
        ArtifactPublicationPoint::BeforeManifest,
        ArtifactPublicationPoint::BeforeIndex,
    ] {
        assert!(matches!(
            store.commit(candidate(&successor), |point| {
                if point == interruption {
                    Err(ArtifactBuildError::Aborted)
                } else {
                    Ok(())
                }
            }),
            Err(ArtifactBuildError::Aborted)
        ));
        let reopened = fixture.store("pair");
        let selected = reopened.lookup_seed(predecessor.seed()).unwrap().unwrap();
        assert_eq!(selected.digest(), predecessor.digest());
        assert_eq!(selected.members().len(), 2);
        for name in ["primary", "secondary"] {
            let value =
                ImageRuntimeCodec::decode_runtime(selected.member_bytes(name).unwrap()).unwrap();
            assert_eq!(value.color_space(), ImageColorSpace::Srgb);
            assert_eq!(value.pixels(), fixture.pixels);
        }
    }
    let published = store
        .commit(candidate(&successor), |_| Ok(()))
        .unwrap()
        .into_group();
    assert_ne!(published.digest(), predecessor.digest());
    let selected = store.lookup_seed(published.seed()).unwrap().unwrap();
    assert_eq!(selected.digest(), published.digest());
    for (name, reference, space) in [
        ("primary", product(), ImageColorSpace::Srgb),
        ("secondary", other, ImageColorSpace::Linear),
    ] {
        let value =
            ImageRuntimeCodec::decode_runtime(selected.member_bytes(name).unwrap()).unwrap();
        assert_eq!(value.product(), Some(reference));
        assert_eq!(value.color_space(), space);
        assert_eq!(value.pixels(), fixture.pixels);
    }
}

#[derive(Default, Clone, Resource)]
struct Published {
    prepared: Option<PreparedImageSnapshot>,
}

#[derive(Default)]
struct InspectPublishedImage;

impl Plugin for InspectPublishedImage {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.durable-image.inspect"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_image::IMAGE_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Published>()?;
        app.add_systems(CoreStage::Update, inspect_published)?;
        Ok(())
    }
}

fn inspect_published(world: &mut World) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<nara_asset::AssetSourceChanges>());
    assert!(!world.contains_resource::<nara_image::ImageReloadStats>());
    let images = world.resource::<Assets<ImageAsset>>();
    assert_eq!(images.len(), 1);
    let (handle, image) = images.iter().next().unwrap();
    assert_eq!(image.product(), Some(product()));
    assert!(image.source().is_none());
    let server = world.resource::<AssetServer>();
    assert_eq!(server.product(handle.id()), Some(product()));
    assert!(server.path(handle.id()).is_none());
    let states = world.resource::<AssetStates>();
    let state = states.state(handle.id()).unwrap();
    assert!(state.usable_version().is_some());
    assert!(state.source_hash().is_none());
    assert!(state.import_recipe_digest().is_none());
    let domains = world.resource::<ProjectDomainAssets>();
    let cpu = domains.collection::<ImageDomain>().unwrap();
    assert_eq!(cpu.source_records().len(), 0);
    assert_eq!(
        cpu.get(&AssetRef::Product(product()))
            .unwrap()
            .pixels()
            .as_ptr(),
        image.pixels().as_ptr()
    );
    let content = ImageContentSnapshot::capture(handle, server, states, images).unwrap();
    let prepared = content.prepared_snapshot();
    assert!(prepared.resource().has_retention_owner());
    assert_eq!(
        prepared.resource().pixels().as_ptr(),
        image.pixels().as_ptr()
    );
    world.resource_mut::<Published>().prepared = Some(prepared);
}

struct Delivery {
    root: tempfile::TempDir,
    recipe: ProductRecipe,
    expected: RuntimeContentExpectation,
}

impl Delivery {
    fn new(group: &VerifiedArtifactGroup) -> Self {
        let root = tempfile::tempdir().unwrap();
        let recipe = ProductRecipe::new()
            .add_runtime_image_codec::<ImageRuntimeCodec>(RuntimeCodecDefinitionId::new(
                "consumer.durable-image.runtime",
                1,
            ))
            .unwrap()
            .add_plugin::<InspectPublishedImage>()
            .unwrap();
        let mut configuration = RuntimeConfiguration::default();
        configuration.runtime.preset = RuntimePreset::LocalHeadless;
        configuration.capabilities.requested = vec![ProductCapability::Runtime2d];
        configuration.tasks.io.workers = 1;
        configuration.tasks.compute.workers = 1;
        configuration.tasks.async_compute.workers = 1;
        let plan = resolve_runtime_configuration(&configuration.resolve().unwrap(), recipe.clone())
            .unwrap();
        let bindings = plan.schema_validation().bindings();
        let sprite = bindings
            .schema(&ComponentTypeId::new("nara.sprite.Sprite"))
            .unwrap();
        let mut value = sprite.authoring_initial_value().unwrap().clone();
        let ComponentValue::Map(fields) = &mut value else {
            panic!("stock Sprite requires a map")
        };
        let Some(ComponentValue::Map(material)) = fields.get_mut("material") else {
            panic!("stock Sprite requires material")
        };
        material.insert(
            "image".into(),
            encode_asset_reference_value(&AssetRef::Product(product())),
        );
        let scene_key =
            SourceDocumentKey::stable_asset(primary(SCENE).source(), SourceDocumentKind::Scene);
        let document = serde_json::json!({
            "kind": "runtime_scene", "format_version": 1, "engine_min_version": "0.1.0",
            "generator": {"name": "durable-image-consumer", "version": "0.0.0"},
            "payload": {
                "source_key": scene_key, "roots": ["stripe"],
                "entities": [{"id": "stripe", "children": [], "components": {
                    "nara.sprite.Sprite": SceneComponentRecord::new(sprite.version(), value)
                }}],
                "provenance": [{"entity": "stripe", "local_entity": "stripe", "origin": {"kind": "local"}}]
            }
        });
        let scene = RuntimeSceneDocumentCandidate::from_json_bytes(
            &serde_json::to_vec(&document).unwrap(),
            RuntimeSceneLimits::default(),
        )
        .unwrap()
        .admit(&scene_key, bindings)
        .unwrap()
        .to_json_bytes()
        .unwrap();
        fs::write(root.path().join("startup.scene"), &scene).unwrap();
        let image = group.member_bytes("runtime").unwrap();
        fs::write(root.path().join("stripe.image"), image).unwrap();
        let image_member = &group.members()[0];
        let members = vec![
            RuntimeContentMember::new(
                "startup.scene",
                ContentDigest::of_bytes(&scene),
                RuntimeContentMemberKind::Scene { source: scene_key },
                vec![primary(SCENE)],
                vec![RuntimeContentDependency::new(
                    product(),
                    RuntimeContentDependencyRole::Required,
                )],
            )
            .unwrap(),
            RuntimeContentMember::new(
                "stripe.image",
                image_member.digest.content(),
                RuntimeContentMemberKind::Asset {
                    format: FormatKind::new(ImageRuntimeCodec::RUNTIME_KIND).unwrap(),
                    version: FormatVersion::ONE,
                },
                image_member.products.clone(),
                vec![],
            )
            .unwrap(),
        ];
        let target = RuntimeContentTarget::new("test-host", "durable-image").unwrap();
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
                startup_scene: primary(SCENE),
                scenes: vec![primary(SCENE)],
                addressable_roots: vec![product()],
                members,
            },
            RuntimeContentCatalogLimits::default(),
        )
        .unwrap();
        let encoded = catalog.to_json_bytes().unwrap();
        fs::write(root.path().join(RUNTIME_CONTENT_CATALOG_FILE), &encoded).unwrap();
        Self {
            root,
            recipe,
            expected: RuntimeContentExpectation::new(ContentDigest::of_bytes(&encoded), target),
        }
    }

    fn open(&self) -> RuntimeContentBase {
        RuntimeContentBase::open(
            &open_explicit_product_project_root(self.root.path()).unwrap(),
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap()
    }

    fn pack(&self, base: &RuntimeContentBase) -> RuntimeContentBase {
        let mut file = File::create(self.root.path().join("content.pack")).unwrap();
        base.write_pack(&mut file).unwrap();
        drop(file);
        RuntimeContentBase::open_pack(
            authority(self.root.path())
                .open_file(&RelativePath::new("content.pack").unwrap())
                .unwrap(),
            &self.expected,
            self.recipe.clone(),
            RuntimeContentLimits::default(),
        )
        .unwrap()
    }
}

fn publish(base: RuntimeContentBase) -> PreparedImageSnapshot {
    let options =
        HeadlessRunOptions::<Published>::frames(NonZeroU32::new(1).unwrap(), Duration::ZERO);
    let mut run = HeadlessRun::from_runtime_content(base, options, Vec::new());
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
                return outcome
                    .prepared
                    .clone()
                    .expect("stock Image must be published before Update");
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "stock Runtime cleanup did not finish"
                );
                std::thread::yield_now();
            }
            HeadlessRunOutcome::Failed => {
                panic!("stock publication failed: {:?}", report.diagnostics())
            }
        }
    }
}

#[test]
fn committed_cook_reaches_catalog_pack_and_stock_native_image_with_independent_old_custody() {
    let fixture = Fixture::new();
    let store = fixture.store("cache");
    let original_import = fixture.import(&store).into_group();
    let original_cook = cook(&store, &original_import, 2048).into_group();
    let original_bytes = original_cook.member_bytes("runtime").unwrap().to_vec();
    let delivery = Delivery::new(&original_cook);
    let base = delivery.open();
    let packed = delivery.pack(&base);
    assert_eq!(base.catalog_digest(), packed.catalog_digest());
    let escaped = packed
        .acquire_asset::<ImageDomain, ImageRuntimeCodec>(product())
        .unwrap();
    let expected = decode(&original_cook).pixels().to_vec();
    assert_eq!(escaped.product().pixels(), expected);
    drop(store);
    fixture.clear_cache("cache");
    fixture.settings(ImageColorSpace::Linear);
    let reopened = fixture.store("cache");
    let new_import = fixture.import(&reopened).into_group();
    let new_cook = cook(&reopened, &new_import, 4096).into_group();
    assert_ne!(new_import.digest(), original_import.digest());
    assert_ne!(new_cook.digest(), original_cook.digest());
    assert_eq!(new_import.seed().source, original_import.seed().source);
    assert_eq!(new_cook.seed().source, original_cook.seed().source);
    assert_eq!(
        original_cook.member_bytes("runtime").unwrap(),
        original_bytes
    );
    assert_eq!(decode(&original_import).pixels(), fixture.pixels);
    assert_eq!(escaped.product().pixels(), expected);
    let new_delivery = Delivery::new(&new_cook);
    let new_base = new_delivery.open();
    let new_packed = new_delivery.pack(&new_base);
    let new_native = publish(new_packed);
    assert_eq!(new_native.resource().color_space(), ImageColorSpace::Linear);
    assert_eq!(new_native.resource().extent(), ImageExtent::new(4096, 1));
    assert_eq!(new_native.resource().pixels(), fixture.pixels);
    for old_base in [base, packed] {
        let native = publish(old_base);
        assert_eq!(native.resource().color_space(), ImageColorSpace::Srgb);
        assert_eq!(native.resource().extent(), ImageExtent::new(2048, 1));
        assert_eq!(native.resource().pixels(), expected);
        assert!(native.resource().has_retention_owner());
    }
    assert_eq!(escaped.product().pixels(), expected);
}

/// The fixture owns a complete valid PNG without another test module or encoder dependency.
/// Its sole zlib block stores one unfiltered RGBA scanline; all PNG CRCs and Adler-32 are real.
fn rgba_png(pixels: &[u8]) -> Vec<u8> {
    assert_eq!(pixels.len(), 4096 * 4);
    let mut scanline = vec![0];
    scanline.extend_from_slice(pixels);
    let length = u16::try_from(scanline.len()).unwrap();
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&length.to_le_bytes());
    zlib.extend_from_slice(&(!length).to_le_bytes());
    zlib.extend_from_slice(&scanline);
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &scanline {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = 4096u32.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&1u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut png, b"IHDR", &ihdr);
    png_chunk(&mut png, b"IDAT", &zlib);
    png_chunk(&mut png, b"IEND", &[]);
    png
}

fn png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    png.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
    let start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let mut crc = !0u32;
    for byte in &png[start..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
}
