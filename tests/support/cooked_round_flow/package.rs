use std::{fs, path::Path};

use nara::{
    fs::ContentDigest,
    project::RuntimeConfiguration,
    project_tooling::host_fs,
    runtime_content::{
        RuntimeContentBase, RuntimeContentCatalog, RuntimeContentCatalogLimits,
        RuntimeContentExpectation, RuntimeContentLimits, RuntimeContentTarget,
    },
};
use serde_json::{Value, json};
use two_level_game::{
    delivery::{ControllerBackend, PlayerSelection, RuntimeKind},
    levels::LevelId,
};

use super::{evidence::number, process::read_bounded};

pub fn feature_closure(graph: &str, player: PlayerSelection) {
    let mut game = false;
    let mut runtime = false;
    let mut ui = false;
    let mut audio = false;
    let mut gamepad = false;
    for line in graph.lines() {
        let (package, features) = line.split_once('|').expect("Cargo's selected tree format");
        let name = package.split_whitespace().next().unwrap();
        let mut features = features
            .trim_end_matches(" (*)")
            .split(',')
            .collect::<Vec<_>>();
        assert!(
            !matches!(
                name,
                "notify" | "nara_asset_watch" | "nara_tooling" | "nara-editor" | "nara_editor"
            ) && !name.starts_with("dear-imgui"),
            "shipping round inherited a development package: {line}"
        );
        match name {
            "two-level-game" => {
                game = true;
                features.sort();
                let mut expected = player.features();
                expected.sort();
                assert_eq!(features, expected);
            }
            "nara" => {
                runtime = true;
                for forbidden in [
                    "default",
                    "project-tooling",
                    "project-workflow",
                    "tooling",
                    "asset-watch",
                ] {
                    assert!(
                        !features.contains(&forbidden),
                        "shipping round inherited {forbidden}: {line}"
                    );
                }
                assert!(features.contains(&"presentation-ui") && features.contains(&"runtime-ui"));
            }
            "nara_ui_render" => ui = true,
            "cpal" => audio = true,
            "nara_gilrs" => gamepad = true,
            _ => {}
        }
    }
    assert!(
        game && runtime && ui,
        "actual shipping round requires game, runtime and presented UI nodes"
    );
    assert_eq!(audio, player.runtime() == RuntimeKind::AudioDesktop);
    assert_eq!(gamepad, player.controllers() == ControllerBackend::Gilrs);
}

pub fn assert_payload(directory: &Path, executable: &str) {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            entry.file_name().into_string().unwrap()
        })
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = vec!["content.pack", "package.json", executable];
    expected.sort();
    assert_eq!(names, expected);
}

pub fn inspect(
    directory: &Path,
    payload: &Value,
    triple: &str,
    configuration: &RuntimeConfiguration,
    player: PlayerSelection,
) {
    let executable = format!("{}.exe", player.binary());
    assert_payload(directory, &executable);
    assert_eq!(payload["executable_name"], executable);
    assert_eq!(payload["pack_file"], "content.pack");
    let selection = &payload["build"]["selection"];
    assert_eq!(selection["package"], "two-level-game");
    assert_eq!(selection["bin"], player.binary());
    assert_eq!(selection["cargo_profile"], "release");
    assert_eq!(selection["target_triple"], triple);
    assert_eq!(selection["default_features"], false);
    let mut features = player.features();
    features.sort();
    assert_eq!(selection["requested_features"], json!(features));
    assert_eq!(payload["build"]["resolved_features"], json!(features));
    for (name, field) in [
        (executable.as_str(), "executable_digest"),
        ("content.pack", "pack_digest"),
    ] {
        let digest = host_fs::readable_file(&directory.join(name))
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap();
        assert_eq!(
            payload[field],
            json!({"length": digest.length(), "hash": digest.as_bytes()})
        );
    }
    let catalog_digest = &payload["catalog_digest"];
    let digest = ContentDigest::from_parts(
        number(catalog_digest, "length"),
        serde_json::from_value(catalog_digest["hash"].clone()).unwrap(),
    );
    let expected = RuntimeContentExpectation::new(
        digest,
        RuntimeContentTarget::new(triple, player.content_profile()).unwrap(),
    );
    let base = RuntimeContentBase::open_pack(
        host_fs::readable_file(&directory.join("content.pack")).unwrap(),
        &expected,
        player.recipe().unwrap(),
        RuntimeContentLimits::default(),
    )
    .unwrap();
    let catalog = base.catalog();
    assert_eq!(base.catalog_digest(), digest);
    assert_eq!(catalog.target().triple(), triple);
    assert_eq!(catalog.target().profile(), player.content_profile());
    assert_eq!(payload["content_target"], json!(catalog.target()));
    assert_eq!(
        catalog.configuration().resolve().unwrap(),
        configuration.resolve().unwrap()
    );
    assert_eq!(catalog.startup_scene(), LevelId::STARTUP.scene_product());
    let mut scenes = LevelId::ALL.map(LevelId::scene_product).to_vec();
    scenes.sort();
    assert_eq!(catalog.scenes(), scenes);
    assert_eq!(catalog.members().len(), 15);
    assert!(catalog.addressable_roots().is_empty());
    assert_eq!(
        catalog.configuration().input.action_map,
        Some(two_level_game::input::input_product())
    );
    assert_eq!(payload["generated_input_name"], "player-bootstrap.rs");
    let generated = &payload["build"]["generated_rust_input"];
    assert_eq!(generated["path"], "player-bootstrap.rs");
    assert_eq!(
        generated["digest"][0],
        payload["generated_input_digest"]["length"]
    );
    assert_eq!(
        generated["digest"][1],
        payload["generated_input_digest"]["hash"]
    );
    let inputs = payload["build"]["declared_inputs"].as_array().unwrap();
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/delivery.rs",
        "src/gameplay/flow.rs",
        "src/gameplay/flow/hud.rs",
        "src/bin/player_desktop/flow.rs",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == required),
            "missing actual round build input: {required}"
        );
    }
    assert!(
        inputs
            .iter()
            .all(|input| input["path"] != "player-bootstrap.rs")
    );
}

pub fn mismatch_catalog(path: &Path, profile: &str) {
    let mut bytes = read_bounded(path, 32 * 1024 * 1024);
    assert_eq!(&bytes[..8], b"NARAPACK");
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
    assert_eq!(catalog.target().profile(), profile);
    let matches = bytes[20..end]
        .windows(profile.len())
        .enumerate()
        .filter_map(|(index, value)| (value == profile.as_bytes()).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let index = 20 + matches[0] + profile.len() - 1;
    assert_eq!(bytes[index], b'g');
    bytes[index] = b'h';
    let changed = RuntimeContentCatalog::from_json_bytes(
        &bytes[20..end],
        RuntimeContentCatalogLimits::default(),
    )
    .unwrap();
    assert_ne!(changed.target(), catalog.target());
    fs::write(path, bytes).unwrap();
}
