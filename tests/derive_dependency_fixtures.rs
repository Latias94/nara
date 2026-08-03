use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn renamed_dependencies_resolve_derive_crates() {
    for fixture in ["renamed-root", "renamed-ecs"] {
        check_fixture(fixture);
    }
}

#[test]
fn reference_game_depends_only_on_the_public_root_package() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = repository.join("reference-game").join("Cargo.toml");
    let output = Command::new(cargo_executable())
        .current_dir(repository)
        .args(["metadata", "--manifest-path"])
        .arg(&manifest)
        .args(["--locked", "--no-deps", "--format-version", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to read reference-game Cargo metadata");

    assert!(
        output.status.success(),
        "reference-game metadata failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata must be valid JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages must be an array");
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0]["name"], "nara-reference-game");

    let dependencies = packages[0]["dependencies"]
        .as_array()
        .expect("package dependencies must be an array");
    assert_eq!(dependencies.len(), 1, "{dependencies:#?}");
    let dependency = &dependencies[0];
    assert_eq!(dependency["name"], "nara");
    assert!(dependency["rename"].is_null());
    assert!(dependency["source"].is_null());
    assert!(dependency["kind"].is_null());
    assert!(dependency["target"].is_null());
    assert_eq!(dependency["optional"], false);
    assert_eq!(dependency["uses_default_features"], false);
    assert_eq!(
        dependency["features"],
        serde_json::json!(["runtime-2d", "serde"])
    );

    let dependency_path = dependency["path"]
        .as_str()
        .map(Path::new)
        .expect("the nara dependency must expose a local path")
        .canonicalize()
        .expect("the nara dependency path must resolve");
    assert_eq!(dependency_path, repository.canonicalize().unwrap());
}

#[test]
fn canonical_project_fixtures_are_checked_out_with_lf() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_paths = [
        "tests/fixtures/formats/v1/component_schema_catalog.json",
        "tests/fixtures/formats/v1/component_schema_catalog.ron",
        "tests/fixtures/formats/v1/prefab.json",
        "tests/fixtures/formats/v1/prefab.ron",
        "tests/fixtures/formats/v1/scene.json",
        "tests/fixtures/formats/v1/scene.ron",
        "tests/fixtures/formats/v1/scene_patch.json",
        "tests/fixtures/formats/v1/scene_patch.ron",
        "tests/fixtures/schema-catalog/lineage-probe-v1.json",
        "tests/fixtures/schema-catalog/lineage-probe-v2.json",
        "reference-game/nara.toml",
        "reference-game/scenes/startup.scene.json",
        "reference-game/prefabs/enemy.prefab.json",
        "reference-game/assets/textures/player.png.meta",
        "reference-game/assets/textures/tiny-dungeon.png.meta",
        "reference-game/schema/component-schema-v1.json",
        "reference-game/schema/component-schema-v2.json",
        "reference-game/schema/component-schema-v3.json",
        "reference-game/schema/component-schema-v4.json",
    ];
    let output = Command::new("git")
        .current_dir(repository)
        .args(["check-attr", "eol", "--"])
        .args(fixture_paths)
        .output()
        .expect("failed to inspect canonical catalog Git attributes");

    assert!(
        output.status.success(),
        "git check-attr failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let attributes = String::from_utf8(output.stdout).expect("Git attribute output must be UTF-8");
    for path in fixture_paths {
        assert!(
            attributes
                .lines()
                .any(|line| line == format!("{path}: eol: lf")),
            "{path} must have the effective Git attribute eol=lf:\n{attributes}"
        );
    }
}

fn check_fixture(fixture: &str) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture_root = repository
        .join("tests")
        .join("fixtures")
        .join("derive-dependencies")
        .join(fixture);
    let manifest = fixture_root.join("Cargo.toml");
    let output = Command::new(cargo_executable())
        .current_dir(repository)
        .args(["check", "--manifest-path"])
        .arg(&manifest)
        .args(["--locked", "--jobs", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", target_directory(repository))
        .output()
        .unwrap_or_else(|error| panic!("failed to run Cargo for {fixture}: {error}"));

    assert!(
        output.status.success(),
        "{fixture} derive dependency fixture failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn target_directory(repository: &Path) -> PathBuf {
    repository.join("target").join("derive-dependency-fixtures")
}
