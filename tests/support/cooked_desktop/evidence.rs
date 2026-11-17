use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use nara::{
    asset::RuntimeAssetCodec,
    fs::ContentDigest,
    image::{ImageExtent, ImageRuntimeCodec},
    project::RuntimeConfiguration,
    project_host::resolve_runtime_configuration,
    runtime_content::{
        RuntimeContentCatalog, RuntimeContentCatalogLimits, RuntimeContentDependencyRole,
        RuntimeContentMemberKind,
    },
    scene::{SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin},
};
use serde_json::{Value, json};
use two_level_game::{
    animation::atlas_product,
    levels::{LevelId, LevelKey},
    sound_bank::{SoundBankCodec, SoundCue, bank_product},
};

use super::{images, process::read_bounded};

pub fn digest(value: ContentDigest) -> Value {
    json!({"length": value.length(), "hash": value.as_bytes()})
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
    assert_eq!(
        names,
        ["content.pack", "package.json", "player_desktop.exe"]
    );
}

pub fn inspect_package(
    directory: &Path,
    payload: &Value,
    triple: &str,
    configuration: &RuntimeConfiguration,
) -> BTreeMap<LevelId, Value> {
    let bytes = read_bounded(&directory.join("content.pack"), 32 * 1024 * 1024);
    assert!(bytes.len() >= 20 && &bytes[..8] == b"NARAPACK");
    assert_eq!(&bytes[8..12], &1_u32.to_le_bytes());
    let length = usize::try_from(u64::from_le_bytes(bytes[12..20].try_into().unwrap())).unwrap();
    let end = 20_usize
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .unwrap();
    let catalog = RuntimeContentCatalog::from_json_bytes(
        &bytes[20..end],
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    assert_eq!(catalog.target().triple(), triple);
    assert_eq!(catalog.target().profile(), "two-level-desktop-shipping");
    let configuration = configuration.resolve().unwrap();
    assert_eq!(catalog.configuration().resolve().unwrap(), configuration);
    let plan =
        resolve_runtime_configuration(&configuration, two_level_game::cooked_desktop_recipe())
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
    assert_eq!(catalog.startup_scene(), LevelId::Meadow.scene_product());
    let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
    scenes.sort();
    assert_eq!(catalog.scenes(), scenes);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(
        catalog.members().len(),
        6,
        "two processed Scenes, three Images and one shared Sound Bank; keyboard declarations are code-first"
    );
    assert_eq!(payload["executable_name"], "player_desktop.exe");
    assert_eq!(payload["pack_file"], "content.pack");
    assert_eq!(
        payload["catalog_digest"],
        digest(ContentDigest::of_bytes(&bytes[20..end]))
    );
    assert_eq!(
        payload["pack_digest"],
        digest(ContentDigest::of_bytes(&bytes))
    );
    assert_eq!(
        payload["executable_digest"],
        digest(ContentDigest::of_bytes(&read_bounded(
            &directory.join("player_desktop.exe"),
            512 * 1024 * 1024
        )))
    );
    let mut offset = end;
    let mut products = BTreeSet::new();
    let mut images = BTreeMap::new();
    for member in catalog.members() {
        assert_eq!(member.products().len(), 1);
        let product = member.products()[0];
        assert!(products.insert(product));
        let length = usize::try_from(member.digest().length()).unwrap();
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .unwrap();
        let data = &bytes[offset..end];
        assert_eq!(ContentDigest::of_bytes(data), member.digest());
        if let Some(level) = LevelId::ALL
            .into_iter()
            .find(|level| level.image_product() == product)
        {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { .. }
            ));
            let image = ImageRuntimeCodec::decode_runtime(data).unwrap();
            assert_eq!(image.product(), Some(product));
            assert_eq!(image.extent(), ImageExtent::new(16, 16));
            assert_eq!(image.pixels().len(), 1024);
            assert!(image.source().is_none());
            images.insert(level, json!({"level": level, "product": product.to_string(), "format": image.format(),
                "color_space": image.color_space(), "extent": image.extent(), "pixel_bytes": image.pixels().len(),
                "pixel_blake3": ContentDigest::of_bytes(image.pixels()).as_bytes(),
                "content_hash": image.content_hash().as_bytes(), "source_free": true}));
        } else if product == atlas_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { .. }
            ));
            images::atlas_proof(&ImageRuntimeCodec::decode_runtime(data).unwrap());
        } else if product == bank_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { .. }
            ));
            let bank = SoundBankCodec::decode_runtime(data).unwrap();
            assert_eq!(bank.product(), product);
            assert!(!bank.samples(SoundCue::Pickup).is_empty());
            assert!(!bank.samples(SoundCue::Travel).is_empty());
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
            let level = LevelId::ALL
                .into_iter()
                .find(|level| level.scene_product() == product)
                .unwrap();
            assert_eq!(
                member
                    .dependencies()
                    .iter()
                    .map(|dependency| {
                        assert_eq!(dependency.role(), RuntimeContentDependencyRole::Required);
                        dependency.product()
                    })
                    .collect::<BTreeSet<_>>(),
                [level.image_product(), atlas_product(), bank_product()]
                    .into_iter()
                    .collect()
            );
        }
        offset = end;
    }
    assert_eq!(offset, bytes.len());
    assert_eq!(images.len(), 2);
    assert_eq!(
        products,
        LevelId::ALL
            .into_iter()
            .flat_map(|level| [level.scene_product(), level.image_product()])
            .chain([atlas_product(), bank_product()])
            .collect()
    );
    images
}

pub fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing unsigned {field}: {value}"))
}

pub fn presentation(record: &Value, previous: Option<&Value>, fallback: bool, changed: bool) {
    let frame = &record["presentation"];
    for field in [
        "runtime_generation",
        "app_frame",
        "render_frame",
        "window_id",
        "surface_generation",
        "backend_instance",
        "device_epoch",
        "queue_submissions",
        "presents",
    ] {
        assert!(
            number(frame, field) > 0,
            "unpresented or incomplete {field}: {record}"
        );
    }
    assert_eq!(record["first_present_verified"], true);
    assert_eq!(
        record["adapter"]["backend"], "dx12",
        "explicit DX12 must fail if unavailable"
    );
    assert!(!record["adapter"]["name"].as_str().unwrap().is_empty());
    let device = record["adapter"]["device_type"].as_str().unwrap();
    if fallback {
        assert_eq!(
            device, "cpu",
            "explicit fallback must select the actual software adapter"
        );
    } else {
        assert!(
            matches!(device, "discrete-gpu" | "integrated-gpu"),
            "physical workflow requires a hardware adapter: {record}"
        );
    }
    if let Some(previous) = previous {
        for field in [
            "runtime_generation",
            "window_id",
            "surface_generation",
            "backend_instance",
            "device_epoch",
        ] {
            assert_eq!(
                frame[field], previous["presentation"][field],
                "original native presentation owner changed"
            );
        }
        assert_eq!(record["adapter"], previous["adapter"]);
        for field in ["app_frame", "render_frame"] {
            let old = number(&previous["presentation"], field);
            let new = number(frame, field);
            assert!(
                if changed { new > old } else { new >= old },
                "game change did not reach a newer actual presentation: {field} {old}->{new}"
            );
        }
    }
}

pub fn progress(record: &Value, level: LevelKey, score: u32, markers: &[u64], visible: usize) {
    assert_eq!(record["level"], json!(level));
    assert_eq!(record["score"], score);
    assert_eq!(record["collected"], score / 10);
    assert_eq!(record["markers"], json!(markers));
    assert_eq!(record["state"]["visible_pickups"], visible);
    assert_eq!(record["state"]["phase"], "Running");
    assert_eq!(record["state"]["initialized"], true);
    assert_eq!(record["state"]["restoring"], false);
    assert!(record["state"]["error"].is_null(), "{record}");
}

pub fn runtime(record: &Value, images: &BTreeMap<LevelId, Value>) {
    assert_eq!(record["success"], true, "{record}");
    assert_eq!(record["source_free"], true);
    assert_eq!(record["processed_runtime"], true);
    assert_eq!(record["content_profile"], "two-level-desktop-shipping");
    for field in ["level", "score", "collected", "markers"] {
        assert_eq!(record[field], record["state"][field]);
    }
    let level: LevelKey = serde_json::from_value(record["level"].clone()).unwrap();
    let resident = [level.level().image_product(), atlas_product()]
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        record["resident_images"],
        json!(
            resident
                .into_iter()
                .map(|product| product.to_string())
                .collect::<Vec<_>>()
        )
    );
    let visited: BTreeSet<LevelId> =
        serde_json::from_value(record["visited_levels"].clone()).unwrap();
    assert!(visited.contains(&level.level()));
    let proofs = record["image_proofs"].as_array().unwrap();
    assert_eq!(proofs.len(), visited.len() + 1);
    let products = record["observed_image_products"].as_array().unwrap();
    assert_eq!(products.len(), visited.len() + 1);
    let atlas = images::atlas_proof(&images::atlas());
    assert_eq!(proofs.iter().filter(|proof| **proof == atlas).count(), 1);
    assert_eq!(
        products
            .iter()
            .filter(|product| **product == atlas_product().to_string())
            .count(),
        1
    );
    for level in visited {
        assert_eq!(
            proofs
                .iter()
                .filter(|proof| **proof == images[&level])
                .count(),
            1
        );
        assert_eq!(
            products
                .iter()
                .filter(|product| **product == level.image_product().to_string())
                .count(),
            1
        );
    }
    let receipts = record["scene_receipts"].as_array().unwrap();
    assert!(!receipts.is_empty() && receipts.len() <= 16);
    let startup = &record["startup_receipt"];
    assert_eq!(receipts[0]["level"], json!(LevelId::Meadow));
    assert_eq!(receipts[0]["receipt"], *startup);
    assert_eq!(
        receipts.last().unwrap()["receipt"],
        record["current_receipt"]
    );
    assert_eq!(
        record["state"]["current_instance"],
        record["current_receipt"]["instance_id"]
    );
    assert_eq!(record["state"]["scene_members"], 6);
    assert!(number(startup, "domain_id") > 0);
    let mut instances = BTreeSet::new();
    for observed in receipts {
        let level: LevelId = serde_json::from_value(observed["level"].clone()).unwrap();
        assert_eq!(observed["scene_product"], level.scene_product().to_string());
        assert_eq!(observed["representation"], "runtime_scene");
        let source: SourceDocumentKey =
            serde_json::from_value(observed["source_key"].clone()).unwrap();
        assert_eq!(source.kind(), SourceDocumentKind::Scene);
        let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
            panic!("lost original ProjectLocation");
        };
        assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
        assert_eq!(path.as_str(), format!("scenes/{}", level.scene_path()));
        let receipt = &observed["receipt"];
        assert_eq!(receipt["domain_id"], startup["domain_id"]);
        assert!(
            number(receipt, "instance_id") > 0 && instances.insert(number(receipt, "instance_id"))
        );
        assert_eq!(receipt["entity_count"], 6);
        assert_eq!(receipt["truncated"], false);
        let mut ids: Vec<String> = serde_json::from_value(receipt["entity_ids"].clone()).unwrap();
        ids.sort();
        assert_eq!(
            ids,
            [
                "camera",
                "checkpoint",
                "explorer",
                "gem-far",
                "gem-near",
                "portal"
            ]
        );
    }
}

pub fn scene_sequence(record: &Value, expected: &[LevelId]) {
    let levels = record["scene_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| serde_json::from_value::<LevelId>(entry["level"].clone()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(levels, expected);
}
