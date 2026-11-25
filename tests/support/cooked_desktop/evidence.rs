use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use nara::{
    asset::RuntimeAssetCodec,
    core::{Color, Vec2},
    fs::ContentDigest,
    image::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageRuntimeCodec},
    input::ActionMapRuntimeCodec,
    material::Material2dRuntimeCodec,
    scene::runtime_configuration::RuntimeConfiguration,
    project_host::resolve_runtime_configuration,
    runtime_content::{
        RuntimeContentCatalog, RuntimeContentCatalogLimits, RuntimeContentDependency,
        RuntimeContentDependencyRole, RuntimeContentMemberKind,
    },
    scene::{SourceDocumentKey, SourceDocumentKind, SourceDocumentOrigin},
    tilemap::{TileAtlasLayout, TileSetRuntimeCodec},
};
use nara_render::animation::SpriteAnimationCodec;
use serde_json::{Value, json};
use two_level_game::{
    animation::{atlas_product, clips_product},
    delivery::PlayerSelection,
    input::input_product,
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    markers::{SCENE_PATH, marker_image_product, marker_product},
    region_material, region_tileset,
    save::Checkpoint,
    side_room,
    sound_bank::{SoundBankCodec, SoundCue, bank_product},
};

#[path = "../two_level_pack.rs"]
mod two_level_pack;

use super::{Request, images, process::read_bounded, walls};

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
    player: PlayerSelection,
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
    assert_eq!(catalog.target().profile(), player.content_profile());
    assert_eq!(payload["content_target"], json!(catalog.target()));
    let configuration = configuration.resolve().unwrap();
    assert_eq!(catalog.configuration().resolve().unwrap(), configuration);
    let plan = resolve_runtime_configuration(&configuration, player.recipe().unwrap()).unwrap();
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
        catalog.configuration().input.action_map,
        Some(input_product())
    );
    assert_eq!(
        catalog.members().len(),
        15,
        "five processed Scenes with two travel roots, five Images, one shared Material, one shared TileSet, one shared SoundBank, one Sprite animation set and one global Input"
    );
    assert_eq!(
        catalog
            .members()
            .iter()
            .filter(|member| matches!(member.kind(), RuntimeContentMemberKind::Scene { .. }))
            .count(),
        5
    );
    assert_eq!(
        catalog
            .members()
            .iter()
            .filter(|member| {
                matches!(
                    member.kind(),
                    RuntimeContentMemberKind::Asset { format, version }
                        if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                            && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
                )
            })
            .count(),
        5
    );
    assert_eq!(payload["executable_name"], "player_desktop.exe");
    assert_eq!(
        catalog
            .members()
            .iter()
            .filter(|member| {
                matches!(
                    member.kind(),
                    RuntimeContentMemberKind::Asset { format, version }
                        if format.as_str() == Material2dRuntimeCodec::RUNTIME_KIND
                            && version.get() == Material2dRuntimeCodec::RUNTIME_VERSION
                )
            })
            .count(),
        1
    );
    assert_eq!(payload["pack_file"], "content.pack");
    assert_eq!(
        catalog
            .members()
            .iter()
            .filter(|member| {
                matches!(
                    member.kind(),
                    RuntimeContentMemberKind::Asset { format, version }
                        if format.as_str() == TileSetRuntimeCodec::RUNTIME_KIND
                            && version.get() == TileSetRuntimeCodec::RUNTIME_VERSION
                )
            })
            .count(),
        1
    );
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
        } else if product == marker_image_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == ImageRuntimeCodec::RUNTIME_KIND
                        && version.get() == ImageRuntimeCodec::RUNTIME_VERSION
            ));
            assert!(member.dependencies().is_empty());
            images::marker_proof(&ImageRuntimeCodec::decode_runtime(data).unwrap());
        } else if product == side_room::image_product() {
            assert!(member.dependencies().is_empty());
            two_level_pack::assert_room_image(&ImageRuntimeCodec::decode_runtime(data).unwrap());
        } else if product == region_material::material_product() {
            let material = Material2dRuntimeCodec::decode_runtime(data).unwrap();
            two_level_pack::assert_room_material_member(member, &material);
        } else if product == region_tileset::tileset_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == TileSetRuntimeCodec::RUNTIME_KIND
                        && version.get() == TileSetRuntimeCodec::RUNTIME_VERSION
            ));
            assert_eq!(
                member.dependencies(),
                [RuntimeContentDependency::new(
                    side_room::image_product(),
                    RuntimeContentDependencyRole::Required,
                )]
            );
            let tileset = TileSetRuntimeCodec::decode_runtime(data).unwrap();
            assert_eq!(tileset.product(), product);
            assert_eq!(tileset.image_product(), side_room::image_product());
            assert_eq!(tileset.atlas(), TileAtlasLayout::grid(Vec2::ONE, 2, 2));
            assert_eq!(tileset.tint(), Color::WHITE);
            tileset
                .validate_image_extent(ImageExtent::new(2, 2))
                .unwrap();
        } else if product == clips_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { format, version }
                    if format.as_str() == SpriteAnimationCodec::RUNTIME_KIND
                        && version.get() == SpriteAnimationCodec::RUNTIME_VERSION
            ));
            assert!(member.dependencies().is_empty());
            let clips = SpriteAnimationCodec::decode_runtime(data).unwrap();
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
            let input = ActionMapRuntimeCodec::decode_runtime(data).unwrap();
            assert_eq!(input.product(), input_product());
            let encoded: Value =
                serde_json::from_slice(&ActionMapRuntimeCodec::encode_runtime(&input).unwrap())
                    .unwrap();
            assert_eq!(encoded["payload"], rebound_input_definition());
        } else if product == bank_product() {
            assert!(matches!(
                member.kind(),
                RuntimeContentMemberKind::Asset { .. }
            ));
            let bank = SoundBankCodec::decode_runtime(data).unwrap();
            assert_eq!(bank.product(), product);
            assert!(!bank.samples(SoundCue::Pickup).is_empty());
            assert!(!bank.samples(SoundCue::Travel).is_empty());
        } else if product == marker_product() {
            assert_eq!(
                member.dependencies(),
                [RuntimeContentDependency::new(
                    marker_image_product(),
                    RuntimeContentDependencyRole::Required,
                )]
            );
            let RuntimeContentMemberKind::Scene { source } = member.kind() else {
                panic!("marker template must be a real cooked Scene");
            };
            assert_eq!(source.kind(), SourceDocumentKind::Scene);
            let SourceDocumentOrigin::ProjectLocation { universe, path } = source.origin() else {
                panic!("marker wrapper must retain its ProjectLocation identity");
            };
            assert_eq!(universe.to_string(), "be3dea0c-93ef-5607-ab84-af715a1f98b3");
            assert_eq!(path.as_str(), format!("scenes/{SCENE_PATH}"));
        } else if [
            side_room::scene_product(),
            side_room::lookout_scene_product(),
        ]
        .contains(&product)
        {
            two_level_pack::assert_room_member(member);
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
            two_level_pack::assert_level_dependencies(member, level);
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
            .chain([
                atlas_product(),
                bank_product(),
                clips_product(),
                input_product(),
                marker_product(),
                marker_image_product(),
                region_material::material_product(),
                region_tileset::tileset_product(),
                side_room::scene_product(),
                side_room::lookout_scene_product(),
                side_room::image_product(),
            ])
            .collect()
    );
    images
}

pub fn number(value: &Value, field: &str) -> u64 {
    value[field]
        .as_u64()
        .unwrap_or_else(|| panic!("missing unsigned {field}: {value}"))
}

pub fn selection(record: &Value, request: &Request) {
    assert_eq!(record["content_profile"], request.content_profile);
    let connected = usize::try_from(number(&record["controllers"], "connected")).unwrap();
    assert_eq!(
        record["controllers"],
        json!({"backend": request.controller_backend, "connected": connected})
    );
    if request.controller_backend == "none" {
        assert_eq!(connected, 0);
    }
}

pub fn tileset(value: &Value, image: &Value) {
    assert_eq!(
        value["product"],
        region_tileset::tileset_product().to_string()
    );
    assert!(number(value, "asset_id") > 0);
    assert_eq!(number(value, "version"), 1);
    assert_eq!(value["observation"], "resident_cpu_declaration");
    let descriptor = region_tileset::descriptor();
    assert_eq!(
        value["declaration"],
        json!({
            "image_product": side_room::image_product().to_string(),
            "atlas": {"tile_size": [1.0_f32, 1.0], "columns": 2, "rows": 2,
                "margin": [0.0_f32, 0.0], "spacing": [0.0_f32, 0.0]},
            "sampler": descriptor.sampler(), "alpha_mode": descriptor.alpha_mode(), "tint": Color::WHITE,
        })
    );
    let captured = &value["image"];
    assert_eq!(captured["asset_id"], image["asset_id"]);
    assert_eq!(captured["version"], image["version"]);
    let expected = ImageAsset::from_product(
        side_room::image_product(),
        ImageExtent::new(2, 2),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        [64, 224, 180, 255].repeat(4),
    )
    .unwrap();
    assert_eq!(captured["product"], side_room::image_product().to_string());
    assert_eq!(captured["extent"], json!(expected.extent()));
    assert_eq!(captured["color_space"], json!(ImageColorSpace::Srgb));
    assert_eq!(
        captured["content_hash"],
        json!(expected.content_hash().as_bytes())
    );
    assert_eq!(captured["pixel_bytes"], 16);
    assert_eq!(captured["source_free"], true);
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

pub fn runtime(record: &Value, request: &Request) {
    assert_eq!(record["success"], true, "{record}");
    assert_eq!(record["source_free"], true);
    assert_eq!(record["processed_runtime"], true);
    assert_eq!(
        record["input_proof"],
        json!({
            "product": input_product().to_string(),
            "source_free": true,
            "definition": rebound_input_definition(),
        })
    );
    selection(record, request);
    for field in ["level", "score", "collected", "markers"] {
        assert_eq!(record[field], record["state"][field]);
    }
    let level: LevelKey = serde_json::from_value(record["level"].clone()).unwrap();
    let mut resident = [
        level.level().image_product(),
        atlas_product(),
        marker_image_product(),
    ]
    .into_iter()
    .collect::<BTreeSet<_>>();
    if ["side_room", "lookout"].into_iter().any(|room| {
        matches!(
            record["state"][room].as_str().unwrap(),
            "open" | "relic_found"
        )
    }) {
        resident.insert(side_room::image_product());
    }
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
    assert_eq!(proofs.len(), visited.len() + 2);
    let products = record["observed_image_products"].as_array().unwrap();
    assert_eq!(products.len(), visited.len() + 2);
    let atlas = images::atlas_proof(&images::atlas());
    assert_eq!(proofs.iter().filter(|proof| **proof == atlas).count(), 1);
    let badge = images::marker_proof(&images::marker_badge());
    assert_eq!(proofs.iter().filter(|proof| **proof == badge).count(), 1);
    assert_eq!(
        products
            .iter()
            .filter(|product| **product == atlas_product().to_string())
            .count(),
        1
    );
    assert_eq!(
        products
            .iter()
            .filter(|product| **product == marker_image_product().to_string())
            .count(),
        1
    );
    for level in visited {
        assert_eq!(
            proofs
                .iter()
                .filter(|proof| **proof == request.images[&level])
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
    assert_eq!(record["state"]["scene_members"], AUTHORED_LEVEL_MEMBERS);
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
        walls::scene(observed);
        assert_eq!(observed["animation_proof"], default_animation_proof());
    }
}

fn rebound_input_definition() -> Value {
    let mut definition = serde_json::from_slice::<Value>(include_bytes!(
        "../../../samples/two-level-game/input/explorer.actions.json"
    ))
    .unwrap()["payload"]
        .clone();
    assert_eq!(definition["bindings"][1]["action"], "two-level-game.move-d");
    assert_eq!(
        definition["bindings"][1]["input"],
        json!({"Key": {"Character": "d"}})
    );
    definition["bindings"][1]["input"]["Key"]["Character"] = json!("j");
    definition
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

pub fn scene_sequence(record: &Value, expected: &[LevelId]) {
    let levels = record["scene_receipts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| serde_json::from_value::<LevelId>(entry["level"].clone()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(levels, expected);
}

pub fn collision_excursions(report: &Value) {
    let probes = report["collision_excursions"].as_array().unwrap();
    assert_eq!(probes.len(), 2);
    let checkpoint: Checkpoint = serde_json::from_value(report["checkpoint"].clone()).unwrap();
    let receipts = report["saved"]["scene_receipts"].as_array().unwrap();
    for (probe, (level, score, marker, inner_x, contact_x)) in probes.iter().zip([
        (LevelId::Meadow, 10, 1_u64, 0.0_f32, -15.01_f32),
        (LevelId::Cavern, 20, 2, 20.0, 4.99),
    ]) {
        let key = LevelKey::main(level);
        assert_eq!(probe["level"], json!(key));
        assert_eq!(probe["score"], score);
        assert_eq!(probe["marker"], marker);
        assert_eq!(probe["inner_wall_x"], json!(inner_x));
        assert_eq!(probe["expected_contact_x"], json!(contact_x));
        assert_eq!(probe["margin"], json!(0.01_f32));
        assert_eq!(probe["held_against_wall_ms"], 350);
        let saved_marker = checkpoint
            .markers
            .iter()
            .find(|saved| saved.id.get() == marker)
            .unwrap();
        assert_eq!(saved_marker.level, key);
        assert_eq!(probe["start"]["player"], json!(saved_marker.position));
        assert!(receipts.iter().any(|receipt| {
            receipt["level"] == json!(level)
                && receipt["receipt"]["instance_id"] == probe["start"]["current_instance"]
        }));
        let stages = [
            "start", "contact", "blocked", "sliding", "cleared", "released",
        ];
        for (index, stage) in stages.into_iter().enumerate() {
            let sample = &probe[stage];
            let player: [f64; 2] = serde_json::from_value(sample["player"].clone()).unwrap();
            assert!(player.into_iter().all(f64::is_finite));
            assert_eq!(
                sample["current_instance"],
                probe["start"]["current_instance"]
            );
            assert_eq!(
                sample["held"],
                match stage {
                    "contact" => json!([false, true, false, false]),
                    "sliding" | "cleared" => json!([false, true, true, false]),
                    _ => json!([false, false, false, false]),
                }
            );
            for field in [
                "runtime_generation",
                "window_id",
                "surface_generation",
                "backend_instance",
                "device_epoch",
            ] {
                assert_eq!(
                    sample["presentation"][field],
                    report["saved"]["presentation"][field]
                );
            }
            if index > 0 {
                let previous = &probe[stages[index - 1]];
                for field in ["tick", "logical_tick", "logical_elapsed_nanos"] {
                    assert!(
                        number(sample, field) > number(previous, field),
                        "collision stage did not advance {field}: {probe}"
                    );
                }
                for field in ["app_frame", "render_frame"] {
                    assert!(
                        number(&sample["presentation"], field)
                            > number(&previous["presentation"], field)
                    );
                }
            }
        }
        let position = |stage: &str| -> [f64; 2] {
            serde_json::from_value(probe[stage]["player"].clone()).unwrap()
        };
        assert!(position("contact")[0] > position("start")[0] + 20.0);
        for stage in ["contact", "blocked", "sliding"] {
            assert!((position(stage)[0] - f64::from(contact_x)).abs() <= 0.03);
        }
        assert_eq!(position("start")[1], 0.0);
        assert_eq!(position("contact")[1], 0.0);
        assert_eq!(position("blocked")[1], 0.0);
        assert!((position("blocked")[0] - position("contact")[0]).abs() <= 0.001);
        assert!(number(&probe["blocked"], "tick") >= number(&probe["contact"], "tick") + 4);
        assert!(
            number(&probe["blocked"], "logical_elapsed_nanos")
                >= number(&probe["contact"], "logical_elapsed_nanos") + 100_000_000
        );
        assert!((10.0..=28.0).contains(&position("sliding")[1]));
        for stage in ["cleared", "released"] {
            let [x, y] = position(stage);
            assert!((f64::from(contact_x) + 2.0..40.0).contains(&x));
            assert!((33.5..96.0).contains(&y));
            assert!(y > position("sliding")[1]);
        }
    }
}
