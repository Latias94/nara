use std::{collections::BTreeSet, fs, ops::Range, path::Path};

use nara::{
    asset::RuntimeAssetCodec,
    fs::ContentDigest,
    image::{ImageExtent, ImageRuntimeCodec},
    project::RuntimeConfiguration,
    project_host::resolve_runtime_configuration,
    runtime_content::{
        RuntimeContentCatalog, RuntimeContentCatalogLimits, RuntimeContentMemberKind,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    levels::LevelId,
    sound_bank::{SoundBankCodec, SoundCue, bank_product},
};

use super::process::read_bounded;

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
) {
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
    assert_eq!(catalog.members().len(), 5);
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
        } else if let Some(level) = LevelId::ALL
            .into_iter()
            .find(|level| level.image_product() == product)
        {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { .. }
            ));
            let image = ImageRuntimeCodec::decode_runtime(&bytes[range]).unwrap();
            assert_eq!(image.product(), Some(level.image_product()));
            assert_eq!(image.extent(), ImageExtent::new(16, 16));
            assert_eq!(image.pixels().len(), 1024);
            assert!(image.source().is_none());
        } else {
            assert!(
                LevelId::ALL
                    .into_iter()
                    .any(|level| level.scene_product() == product)
            );
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
        }
    }
    assert_eq!(products.len(), 5);
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
