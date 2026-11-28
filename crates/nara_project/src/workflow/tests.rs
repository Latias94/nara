use std::{collections::BTreeSet, fs, path::Path, process::ExitStatus};

use nara_asset::{AssetScanMode, AssetSourceKind, AssetSourcePath, ProjectAssetDatabase};
use nara_core::AssetId;
use nara_reflect::ComponentSchemaCatalog;

use super::{
    ProjectTrust, SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, create_project,
    read_schema_cache, refresh_schema, schema::finish_refresh,
};
use crate::{
    EditorPlayMode, ProjectManifest,
    play::{BuildError, BuildRequest},
};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt as _;
#[cfg(windows)]
use std::os::windows::process::ExitStatusExt as _;

fn local_checkout(root: &Path) {
    fs::create_dir_all(root.join("vendor/winit")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        r#"
[package]
name = "nara"
edition = "2024"
rust-version = "1.95"

[patch.crates-io]
wgpu = { git = "https://example.com/wgpu", rev = "pinned-revision" }
winit = { path = "vendor/winit" }
"#,
    )
    .unwrap();
    fs::write(root.join("vendor/winit/Cargo.toml"), "").unwrap();
}

#[test]
fn starter_uses_the_checkout_patch_sources_and_a_code_first_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("engine with spaces");
    let project = temp.path().join("my-game");
    local_checkout(&checkout);
    create_project(&project, "my-game", &checkout).unwrap();

    let cargo: toml::Value =
        toml::from_str(&fs::read_to_string(project.join("Cargo.toml")).unwrap()).unwrap();
    let dependency = &cargo["dependencies"]["nara"];
    let patches = &cargo["patch"]["crates-io"];
    assert_eq!(dependency["path"].as_str(), checkout.to_str());
    assert_eq!(
        patches["winit"]["path"].as_str(),
        checkout.join("vendor/winit").to_str()
    );
    assert_eq!(patches["wgpu"]["rev"].as_str(), Some("pinned-revision"));
    assert_eq!(cargo["dependencies"].as_table().unwrap().len(), 1);
    let desktop = cargo["features"]["desktop"].as_array().unwrap();
    let source = cargo["features"]["source"].as_array().unwrap();
    assert!(
        desktop
            .iter()
            .any(|feature| feature.as_str() == Some("nara/scene-cooked"))
    );
    assert!(
        desktop
            .iter()
            .any(|feature| feature.as_str() == Some("nara/asset-cooked"))
    );
    for required in ["nara/scene-cook", "nara/asset-image-import"] {
        assert!(source.iter().any(|feature| feature.as_str() == Some(required)));
    }
    assert!(project.join("src/bin/nara_schema.rs").is_file());
    assert!(!project.join("src/source.rs").exists());

    let manifest = super::check_project(&project).unwrap();
    assert_eq!(manifest.project.name, "my-game");
    assert_eq!(manifest.project.version, "0.1.0");
    assert_eq!(manifest.paths.assets, "assets");
    assert!(manifest.window.is_none());
    assert_eq!(manifest.editor.play, EditorPlayMode::External);
    assert_eq!(manifest.delivery.unwrap().scenes.len(), 1);
}

#[test]
fn starters_generate_fresh_ids_and_resolve_their_own_assets() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("engine");
    local_checkout(&checkout);
    let mut generated = BTreeSet::new();
    for directory in ["first", "second"] {
        let project = temp.path().join(directory);
        create_project(&project, "my-game", &checkout).unwrap();

        let assets = ProjectAssetDatabase::scan(
            &project,
            &[AssetSourcePath::new("assets").unwrap()],
            AssetScanMode::ReadOnly,
        )
        .unwrap();
        assert_eq!(assets.len(), 3);
        let asset_ids = [
            ("assets/main.scene.ron", AssetSourceKind::Scene),
            ("assets/sprite.prefab.ron", AssetSourceKind::Prefab),
            ("assets/white.png", AssetSourceKind::Image),
        ]
        .map(|(path, kind)| {
            let path = AssetSourcePath::new(path).unwrap();
            let record = assets.record_for_path(&path).unwrap();
            assert_eq!(record.source_kind(), &kind);
            let id = AssetId::try_from(record.stable_id()).unwrap();
            assert_eq!(id.as_uuid().get_version_num(), 7);
            assert!(generated.insert(id.as_uuid()));
            assert_eq!(
                assets.record_for_stable_id(id.into()).unwrap().path(),
                &path
            );
            id
        });
        let manifest = super::read_project_manifest(&project).unwrap();
        assert_eq!(manifest.delivery.unwrap().scenes, [asset_ids[0]]);
        let main = fs::read_to_string(project.join("src/main.rs")).unwrap();
        assert!(main.contains(&format!("const ENTRY_SCENE: &str = \"{}\";", asset_ids[0])));
        assert!(
            fs::read(project.join("assets/white.png"))
                .unwrap()
                .starts_with(b"\x89PNG\r\n\x1a\n")
        );

        #[cfg(feature = "delivery")]
        {
            use nara_scene::authoring::{SceneDocument, SpriteSource};

            let scene = SceneDocument::read(project.join("assets/main.scene.ron")).unwrap();
            let prefab = SceneDocument::read(project.join("assets/sprite.prefab.ron")).unwrap();
            for document in [&scene, &prefab] {
                assert_eq!(document.entities.len(), 1);
                for id in [document.id.as_uuid(), document.entities[0].id.as_uuid()] {
                    assert_eq!(id.get_version_num(), 7);
                    assert!(generated.insert(id));
                }
            }
            assert_eq!(scene.assets["sprite"].id, asset_ids[1]);
            assert_eq!(
                scene.entities[0].prefab.as_ref().unwrap().asset,
                asset_ids[1]
            );
            assert_eq!(prefab.assets["white"].id, asset_ids[2]);
            let sprite = prefab.entities[0].typed::<SpriteSource>().unwrap().unwrap();
            assert_eq!(sprite.image, Some(asset_ids[2]));
        }
    }
}

#[test]
fn existing_project_is_never_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("engine");
    local_checkout(&checkout);
    let existing_manifest = "existing manifest";
    fs::write(temp.path().join("nara.toml"), existing_manifest).unwrap();

    assert!(create_project(temp.path(), "existing", &checkout).is_err());
    assert_eq!(
        fs::read_to_string(temp.path().join("nara.toml")).unwrap(),
        existing_manifest
    );
    assert!(!temp.path().join("src").exists());
}

#[test]
fn manifest_accepts_portable_paths_and_partial_window_metadata() {
    let manifest = ProjectManifest::parse_toml(
        r#"
[project]
name = "My Game"
version = "0.2.0-dev"
[paths]
assets = "data/art"
[window]
title = "A Window"
width = 960
"#,
    )
    .unwrap();
    assert_eq!(manifest.paths.assets, "data/art");
    let window = manifest.window.unwrap();
    assert_eq!(window.title.as_deref(), Some("A Window"));
    assert_eq!(window.width.unwrap().get(), 960);
    assert!(window.height.is_none());
}

#[test]
fn snapshot_play_requires_explicit_project_opt_in() {
    let project = "[project]\nname = 'My Game'\nversion = '0.1.0'\n";
    for (editor, mode) in [
        ("", EditorPlayMode::External),
        ("[editor]\n", EditorPlayMode::External),
        ("[editor]\nplay = 'external'\n", EditorPlayMode::External),
        ("[editor]\nplay = 'snapshot'\n", EditorPlayMode::Snapshot),
    ] {
        let manifest = ProjectManifest::parse_toml(&format!("{project}{editor}")).unwrap();
        assert_eq!(manifest.editor.play, mode);
    }
    assert!(ProjectManifest::parse_toml(&format!("{project}[editor]\nplay = 'auto'\n")).is_err());
}

#[test]
fn check_validates_manifests_before_discovering_assets() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let source = "[project]\nname = 'My Game'\nversion = '0.1.0'\n";
    let nara_manifest = project.join("nara.toml");
    let cargo_manifest = project.join("Cargo.toml");
    fs::write(&nara_manifest, source).unwrap();
    fs::write(
        &cargo_manifest,
        "[package]\nname = 'my-game'\ndefault-run = 'demo'\n\
         [[bin]]\nname = 'demo'\nrequired-features = ['desktop']\n",
    )
    .unwrap();
    super::check_project(project).unwrap();
    let package = super::CargoPackage::read(project).unwrap();
    assert_eq!(package.name(), "my-game");
    assert_eq!(package.default_binary(), "demo");
    assert_eq!(package.required_features("demo"), ["desktop"]);

    for invalid_section in [
        "\n[paths]\nassets = '../outside'",
        "\n[paths]\nassets = 'art\\sprites'",
        "\n[window]\nwidth = 0",
        "\n[window]\nheight = 4294967296",
        "\n[window]\ntitle = ' '",
        "\n[profiles.desktop]",
    ] {
        fs::write(&nara_manifest, format!("{source}{invalid_section}\n")).unwrap();
        assert!(matches!(
            super::check_project(project),
            Err(super::ProjectCheckError::Parse { path, .. }) if path == nara_manifest
        ));
    }
    fs::write(&nara_manifest, source).unwrap();
    fs::write(&cargo_manifest, "[workspace]\n").unwrap();
    assert!(matches!(
        super::check_project(project),
        Err(super::ProjectCheckError::Parse { path, .. }) if path == cargo_manifest
    ));
}

#[test]
fn check_discovers_and_reuses_asset_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    fs::write(
        project.join("nara.toml"),
        "[project]\nname = 'My Game'\nversion = '0.1.0'\n[paths]\nassets = 'content'\n",
    )
    .unwrap();
    fs::write(project.join("Cargo.toml"), "[package]\nname = 'my-game'\n").unwrap();
    fs::create_dir(project.join("content")).unwrap();
    fs::write(project.join("content/player.png"), []).unwrap();

    super::check_project(project).unwrap();
    let meta = project.join("content/player.png.meta");
    let admitted = fs::read(&meta).unwrap();
    super::check_project(project).unwrap();
    assert_eq!(fs::read(meta).unwrap(), admitted);
}

#[test]
fn untrusted_schema_refresh_stops_before_building() {
    let temp = tempfile::tempdir().unwrap();
    let request = BuildRequest::new(temp.path(), "my-game", SCHEMA_BINARY);
    let error = refresh_schema(&request, ProjectTrust::Untrusted, |_| {
        panic!("the build must not start")
    })
    .unwrap_err();
    assert!(matches!(error, SchemaCacheError::UntrustedProject));
    assert!(read_schema_cache(temp.path()).unwrap().is_none());
}

#[test]
fn a_schema_request_cannot_select_a_manifest_outside_the_trusted_root() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let outside_manifest = temp.path().join("Cargo.toml");
    fs::create_dir(&project).unwrap();
    fs::write(&outside_manifest, "").unwrap();
    let request = BuildRequest {
        manifest_path: &outside_manifest,
        ..BuildRequest::new(&project, "my-game", SCHEMA_BINARY)
    };
    let error = refresh_schema(&request, ProjectTrust::Trusted, |_| {
        panic!("the build must not start")
    })
    .unwrap_err();
    assert!(matches!(
        error,
        SchemaCacheError::Build(BuildError::ManifestOutsideProject)
    ));
}

#[test]
fn only_successful_valid_producer_output_replaces_the_schema_cache() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let cache_path = root.join(SCHEMA_CACHE_PATH);
    let success = ExitStatus::from_raw(0);
    let failure = ExitStatus::from_raw(256);
    let catalog = ComponentSchemaCatalog::default();
    let json = catalog.to_json_string().unwrap();

    assert_eq!(read_schema_cache(root).unwrap(), None);
    assert_eq!(
        finish_refresh(root, success, json.as_bytes(), || false).unwrap(),
        catalog
    );
    assert_eq!(read_schema_cache(root).unwrap(), Some(catalog));

    assert!(matches!(
        finish_refresh(root, failure, json.as_bytes(), || false),
        Err(SchemaCacheError::ProducerFailed(_))
    ));
    assert!(matches!(
        finish_refresh(root, success, b"{}", || false),
        Err(SchemaCacheError::InvalidCatalog(_))
    ));
    let mut cancelled = false;
    let replacement = format!("{json}\r\n");
    assert!(matches!(
        finish_refresh(root, success, replacement.as_bytes(), || {
            let previous = cancelled;
            cancelled = true;
            previous
        }),
        Err(SchemaCacheError::Interrupted)
    ));
    assert_eq!(fs::read_to_string(&cache_path).unwrap(), json);

    finish_refresh(root, success, replacement.as_bytes(), || false).unwrap();
    assert_eq!(fs::read(cache_path).unwrap(), replacement.as_bytes());
}
