use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ops::Range,
    path::Path,
};

use nara::{
    asset::RuntimeAssetCodec,
    fs::ContentDigest,
    image::{ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec},
    input::ActionMapRuntimeCodec,
    project::RuntimeConfiguration,
    project_host::resolve_runtime_configuration,
    runtime_content::{
        RuntimeContentCatalog, RuntimeContentCatalogLimits, RuntimeContentDependencyRole,
        RuntimeContentMemberKind,
    },
    scene::{SourceDocumentKind, SourceDocumentOrigin},
};
use nara_sprite_animation::SpriteAnimationCodec;
use serde_json::{Value, json};
use two_level_game::{
    animation::{atlas_product, clips_product},
    input::input_product,
    levels::LevelId,
    markers::{SCENE_PATH, marker_product},
    sound_bank::{SoundBankCodec, SoundCue, bank_product},
};

use super::{images, process::read_bounded};

struct Pack {
    catalog_bytes: Range<usize>,
    catalog: RuntimeContentCatalog,
    members: Vec<Range<usize>>,
}

fn inspect_pack(bytes: &[u8]) -> Pack {
    assert!(bytes.len() >= 20 && bytes.len() <= 32 * 1024 * 1024);
    assert_eq!(&bytes[..8], b"NARAPACK");
    assert_eq!(&bytes[8..12], &1_u32.to_le_bytes());
    let length = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap())).unwrap();
    let end = 20_usize
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .unwrap();
    let catalog_bytes = 20..end;
    let catalog = RuntimeContentCatalog::from_json_bytes(
        &bytes[catalog_bytes.clone()],
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    let mut offset = end;
    let members = catalog
        .members()
        .iter()
        .map(|member| {
            let length = usize::try_from(member.digest().length()).unwrap();
            let end = offset
                .checked_add(length)
                .filter(|end| *end <= bytes.len())
                .unwrap();
            let range = offset..end;
            assert_eq!(
                ContentDigest::of_bytes(&bytes[range.clone()]),
                member.digest()
            );
            offset = end;
            range
        })
        .collect();
    assert_eq!(offset, bytes.len());
    Pack {
        catalog_bytes,
        catalog,
        members,
    }
}

pub fn digest(value: ContentDigest) -> Value {
    json!({"length": value.length(), "hash": value.as_bytes()})
}

pub fn sample_hash(samples: &[f32]) -> Value {
    let bytes = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect::<Vec<_>>();
    json!(ContentDigest::of_bytes(&bytes).as_bytes())
}

pub fn assert_payload(directory: &Path) {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, ["content.pack", "package.json", "player_audio.exe"]);
}

pub fn inspect(
    directory: &Path,
    payload: &Value,
    triple: &str,
    configuration: &RuntimeConfiguration,
    expected: &[Vec<f32>; 2],
) -> BTreeMap<String, Value> {
    let bytes = read_bounded(&directory.join("content.pack"), 32 * 1024 * 1024);
    let pack = inspect_pack(&bytes);
    let catalog = &pack.catalog;
    assert_eq!(catalog.target().triple(), triple);
    assert_eq!(
        catalog.target().profile(),
        "two-level-audio-desktop-shipping"
    );
    let configuration = configuration.resolve().unwrap();
    assert_eq!(catalog.configuration().resolve().unwrap(), configuration);
    let plan =
        resolve_runtime_configuration(&configuration, two_level_game::audio::cooked_recipe())
            .unwrap();
    assert_eq!(
        catalog.plugin_plan_fingerprint(),
        plan.plugin_plan().fingerprint().as_bytes()
    );
    assert_eq!(
        catalog.schema_fingerprint(),
        plan.schema_validation()
            .composition_fingerprint()
            .as_bytes()
    );
    assert_eq!(
        catalog.runtime_codecs_fingerprint(),
        plan.runtime_codecs_fingerprint().as_bytes()
    );
    assert_eq!(catalog.startup_scene(), LevelId::STARTUP.scene_product());
    let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
    scenes.sort();
    assert_eq!(catalog.scenes(), scenes);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(catalog.members().len(), 9);
    assert_eq!(
        catalog
            .members()
            .iter()
            .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
            .count(),
        3
    );
    assert_eq!(
        catalog.configuration().input.action_map,
        Some(input_product())
    );
    assert_eq!(payload["executable_name"], "player_audio.exe");
    assert_eq!(payload["pack_file"], "content.pack");
    assert_eq!(
        payload["catalog_digest"],
        digest(ContentDigest::of_bytes(&bytes[pack.catalog_bytes.clone()]))
    );
    assert_eq!(
        payload["pack_digest"],
        digest(ContentDigest::of_bytes(&bytes))
    );
    assert_eq!(
        payload["executable_digest"],
        digest(ContentDigest::of_bytes(&read_bounded(
            &directory.join("player_audio.exe"),
            512 * 1024 * 1024
        )))
    );
    let selection = &payload["build"]["selection"];
    assert_eq!(selection["package"], "two-level-game");
    assert_eq!(selection["bin"], "player_audio");
    assert_eq!(selection["cargo_profile"], "release");
    assert_eq!(selection["target_triple"], triple);
    assert_eq!(selection["default_features"], false);
    let mut features = selection["requested_features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|feature| feature.as_str().unwrap())
        .collect::<Vec<_>>();
    features.sort();
    assert_eq!(features, ["audio-cpal", "cooked-player", "desktop"]);
    let mut products = BTreeSet::new();
    let mut images = BTreeMap::new();
    for (member, range) in catalog.members().iter().zip(pack.members) {
        assert_eq!(member.products().len(), 1);
        let product = member.products()[0];
        assert!(products.insert(product));
        if product == bank_product() {
            assert!(
                matches!(member.kind(), RuntimeContentMemberKind::Asset { format, version }
                if format.as_str() == SoundBankCodec::RUNTIME_KIND && version.get() == SoundBankCodec::RUNTIME_VERSION)
            );
            assert!(member.dependencies().is_empty());
            let bank = SoundBankCodec::decode_runtime(&bytes[range]).unwrap();
            assert_eq!(bank.product(), bank_product());
            assert_eq!(bank.samples(SoundCue::Pickup), expected[0]);
            assert_eq!(bank.samples(SoundCue::Travel), expected[1]);
        } else if product == clips_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == SpriteAnimationCodec::RUNTIME_KIND
                        && version.get() == SpriteAnimationCodec::RUNTIME_VERSION
            ));
            assert!(member.dependencies().is_empty());
            let clips = SpriteAnimationCodec::decode_runtime(&bytes[range]).unwrap();
            assert_eq!(clips.product(), product);
            let encoded: Value =
                serde_json::from_slice(&SpriteAnimationCodec::encode_runtime(&clips).unwrap())
                    .unwrap();
            assert_eq!(encoded["clips"], default_animation_proof()["clips"]);
        } else if product == input_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == ActionMapRuntimeCodec::RUNTIME_KIND
                        && version.get() == ActionMapRuntimeCodec::RUNTIME_VERSION
            ));
            assert!(member.dependencies().is_empty());
            let input = ActionMapRuntimeCodec::decode_runtime(&bytes[range]).unwrap();
            assert_eq!(input.product(), input_product());
            let encoded: Value =
                serde_json::from_slice(&ActionMapRuntimeCodec::encode_runtime(&input).unwrap())
                    .unwrap();
            assert_eq!(encoded["payload"], default_input_definition());
        } else if let Some(level) = LevelId::ALL
            .into_iter()
            .find(|level| level.image_product() == product)
        {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                        && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
            ));
            let image = ImageRuntimeCodec::decode_runtime(&bytes[range]).unwrap();
            assert_eq!(image.product(), Some(level.image_product()));
            assert_eq!(image.extent(), ImageExtent::new(16, 16));
            assert_eq!(image.format(), ImageFormat::Rgba8);
            assert_eq!(image.color_space(), ImageColorSpace::Srgb);
            assert_eq!(image.pixels().len(), 1024);
            assert!(image.source().is_none());
            images.insert(
                product.to_string(),
                json!({
                    "level": level,
                    "product": product.to_string(),
                    "format": image.format(),
                    "color_space": image.color_space(),
                    "extent": image.extent(),
                    "pixel_bytes": image.pixels().len(),
                    "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
                    "content_hash": image.content_hash().as_bytes(),
                    "source_free": true,
                }),
            );
        } else if product == atlas_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                        && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
            ));
            assert!(member.dependencies().is_empty());
            let image = ImageRuntimeCodec::decode_runtime(&bytes[range]).unwrap();
            assert!(
                images
                    .insert(product.to_string(), images::atlas_proof(&image))
                    .is_none()
            );
        } else if product == marker_product() {
            assert!(member.dependencies().is_empty());
            let RuntimeContentMemberKind::Scene { source } = member.kind() else {
                panic!("marker template must be a real cooked Scene");
            };
            assert_eq!(source.kind(), SourceDocumentKind::Scene);
            let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
                panic!("marker wrapper must retain its ProjectLocation identity");
            };
            assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
            assert_eq!(path.as_str(), format!("scenes/{SCENE_PATH}"));
        } else {
            let level = LevelId::ALL
                .into_iter()
                .find(|level| level.scene_product() == product)
                .expect("only the two declared Scene Products remain");
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Scene { .. }
            ));
            assert!(
                member
                    .dependencies()
                    .iter()
                    .any(|dependency| dependency.product() == bank_product())
            );
            assert_eq!(member.dependencies().len(), 5);
            assert_eq!(
                member
                    .dependencies()
                    .iter()
                    .map(|dependency| dependency.product())
                    .collect::<BTreeSet<_>>(),
                [
                    level.image_product(),
                    atlas_product(),
                    bank_product(),
                    clips_product(),
                    marker_product(),
                ]
                .into_iter()
                .collect()
            );
            assert!(
                member.dependencies().iter().all(|dependency| {
                    dependency.role() == RuntimeContentDependencyRole::Required
                })
            );
        }
    }
    assert_eq!(images.len(), 3);
    assert_eq!(
        products,
        LevelId::ALL
            .into_iter()
            .flat_map(|level| [level.scene_product(), level.image_product()])
            .chain([
                atlas_product(),
                bank_product(),
                clips_product(),
                input_product(),
                marker_product(),
            ])
            .collect()
    );
    images
}

pub fn assert_input_proof(record: &Value) {
    assert_eq!(
        record["input_proof"],
        json!({
            "product": input_product().to_string(),
            "source_free": true,
            "definition": default_input_definition(),
        })
    );
}

fn default_input_definition() -> Value {
    serde_json::from_slice::<Value>(include_bytes!(
        "../../../samples/two-level-game/input/explorer.actions.json"
    ))
    .unwrap()["payload"]
        .clone()
}

pub fn assert_animation_proof(observed: &Value) {
    assert_eq!(observed["animation_proof"], default_animation_proof());
}

fn default_animation_proof() -> Value {
    json!({
        "product": clips_product().to_string(),
        "source_free": true,
        "clips": [
            {"name": "idle", "mode": "once", "frames": [
                {"min": [0.0_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
            ]},
            {"name": "walk", "mode": "loop", "frames": [
                {"min": [0.25_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                {"min": [0.5_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
                {"min": [0.75_f32, 0.0_f32], "size": [0.25_f32, 1.0_f32], "duration_nanos": 100_000_000_u64},
            ]},
        ],
    })
}

pub fn copy_package(source: &Path, destination: &Path) {
    assert_payload(source);
    assert!(fs::read_dir(destination).unwrap().next().is_none());
    for name in ["player_audio.exe", "content.pack", "package.json"] {
        fs::copy(source.join(name), destination.join(name)).unwrap();
    }
    assert_payload(destination);
}

pub fn corrupt_bank(path: &Path) {
    let mut bytes = read_bounded(path, 32 * 1024 * 1024);
    let pack = inspect_pack(&bytes);
    let index = pack
        .catalog
        .members()
        .iter()
        .position(|member| member.products() == [bank_product()])
        .unwrap();
    let range = &pack.members[index];
    assert!(range.len() > 64);
    bytes[range.start + range.len() / 2] ^= 1;
    assert_ne!(
        ContentDigest::of_bytes(&bytes[range.clone()]),
        pack.catalog.members()[index].digest()
    );
    fs::write(path, bytes).unwrap();
}

pub fn mismatch_catalog(path: &Path) {
    let mut bytes = read_bounded(path, 32 * 1024 * 1024);
    let pack = inspect_pack(&bytes);
    let original = ContentDigest::of_bytes(&bytes[pack.catalog_bytes.clone()]);
    let profile = b"two-level-audio-desktop-shipping";
    let matches = bytes[pack.catalog_bytes.clone()]
        .windows(profile.len())
        .enumerate()
        .filter_map(|(index, value)| (value == profile).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let offset = pack.catalog_bytes.start + matches[0] + profile.len() - 1;
    bytes[offset] = b'h';
    assert_ne!(
        ContentDigest::of_bytes(&bytes[pack.catalog_bytes.clone()]),
        original
    );
    let changed = RuntimeContentCatalog::from_json_bytes(
        &bytes[pack.catalog_bytes],
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    assert_ne!(changed.target(), pack.catalog.target());
    fs::write(path, bytes).unwrap();
}
