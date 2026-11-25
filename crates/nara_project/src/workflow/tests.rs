use std::{fs, path::Path, process::ExitStatus};

use nara_reflect::ComponentSchemaCatalog;

use super::{
    ProjectTrust, SCHEMA_BINARY, SCHEMA_CACHE_PATH, SchemaCacheError, create_project,
    read_schema_cache, refresh_schema, schema::finish_refresh,
};
use crate::{ProjectManifest, play::BuildRequest};

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
    assert!(project.join("src/bin/nara_schema.rs").is_file());

    let project_source = fs::read_to_string(project.join("nara.toml")).unwrap();
    let manifest = ProjectManifest::parse_toml_str(&project_source);
    assert!(!manifest.has_errors(), "{manifest:?}");
    assert!(manifest.manifest.unwrap().startup.default_scene.is_none());
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
fn check_compiles_the_selected_ron_action_map_without_cargo() {
    let temp = tempfile::tempdir().unwrap();
    let checkout = temp.path().join("engine");
    let project = temp.path().join("my-game");
    local_checkout(&checkout);
    create_project(&project, "my-game", &checkout).unwrap();

    let selected = "input/default.actions.ron";
    let manifest = project.join("nara.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(
        manifest,
        format!("{source}\n[input]\naction_map = '{selected}'\n"),
    )
    .unwrap();
    fs::create_dir(project.join("input")).unwrap();
    let path = project.join(selected);
    let map = r#"(
        kind: "action_map",
        format_version: 1,
        engine_min_version: "0.1.0",
        generator: (name: "fixture", version: "0.1.0"),
        payload: (
            source: "b23d8f3a-9770-5c1a-bf4d-81fb93171257",
            contexts: [(context: "gameplay", enabled: true)],
            bindings: [(action: "jump", input: Key(Space), context: "gameplay")],
        ),
    )"#;
    fs::write(&path, map).unwrap();
    super::check_project(&project, None).unwrap();
    fs::write(path, map.replace("Key(Space)", "Key(UnknownKey)")).unwrap();
    assert!(matches!(
        super::check_project(&project, None),
        Err(super::ProjectCheckError::Source { path, .. }) if path == selected
    ));
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
    assert!(matches!(error, SchemaCacheError::ManifestOutsideProject));
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
