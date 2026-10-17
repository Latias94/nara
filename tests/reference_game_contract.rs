use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "support/reference_game_manifest.rs"]
mod reference_game_manifest;

use reference_game_manifest::REFERENCE_GAME_MANIFEST_DEPENDENCIES;

#[test]
fn reference_game_uses_the_root_facade_for_production_engine_code() {
    let root = reference_game_root();
    let manifest_source = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let manifest = toml::from_str::<toml::Value>(&manifest_source).unwrap();
    let mut dependencies = Vec::new();
    collect_dependency_entries(&manifest, &mut Vec::new(), &mut dependencies);
    dependencies.sort();

    assert_eq!(dependencies, REFERENCE_GAME_MANIFEST_DEPENDENCIES);
    let nara = manifest
        .get("dependencies")
        .and_then(|dependencies| dependencies.get("nara"))
        .and_then(toml::Value::as_table)
        .expect("the reference game must depend on the public root crate");
    assert_eq!(nara.get("path").and_then(toml::Value::as_str), Some(".."));
    assert_eq!(
        nara.get("default-features").and_then(toml::Value::as_bool),
        Some(false)
    );
    assert_eq!(
        nara.get("features")
            .and_then(toml::Value::as_array)
            .unwrap()
            .iter()
            .map(|feature| feature.as_str().unwrap())
            .collect::<Vec<_>>(),
        ["runtime-2d", "runtime-ui", "serde", "project-tooling"]
    );
    assert_eq!(
        manifest["features"]["desktop"]
            .as_array()
            .unwrap()
            .iter()
            .map(|feature| feature.as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "nara/presentation-2d",
            "nara/presentation-ui",
            "nara/desktop-winit",
            "nara/render-wgpu",
        ]
    );
    assert!(!manifest_source.contains("[patch"));
    assert!(!manifest_source.contains("workspace = true"));
    assert!(root.join("Cargo.lock").is_file());
}

fn reference_game_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("reference-game")
}

fn collect_dependency_entries(
    value: &toml::Value,
    path: &mut Vec<String>,
    entries: &mut Vec<String>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, child) in table {
        path.push(key.clone());
        if matches!(
            key.as_str(),
            "dependencies" | "dev-dependencies" | "build-dependencies"
        ) {
            let dependencies = child
                .as_table()
                .expect("Cargo dependency sections must be TOML tables");
            for dependency in dependencies.keys() {
                entries.push(format!("{}.{}", path.join("."), dependency));
            }
        } else {
            collect_dependency_entries(child, path, entries);
        }
        path.pop();
    }
}
