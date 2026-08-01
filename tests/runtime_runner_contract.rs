use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

const FIXTURE_PACKAGE: &str = "renamed-nara-runtime-runner-fixture";
const FIXTURE_RELATIVE_PATH: &str = "tests/fixtures/runtime-runner/renamed-root";
const REQUIRED_PRODUCT_SURFACE: [&str; 6] = [
    "ProductRecipe",
    "ProductConfiguration",
    "SchemaContribution",
    "add_plugin",
    "add_contribution",
    "HeadlessRun::from_recipe",
];
const FORBIDDEN_PRODUCT_SURFACE: [&str; 15] = [
    "PluginDefinition",
    "with_schema_provider",
    "RuntimeAdmissionReservation",
    "RuntimeCandidate",
    "RuntimeInstance",
    "RuntimeObligationLedger",
    "RuntimeRetirement",
    "run_once",
    "World",
    "#[path",
    "include!",
    "include_str!",
    "include_bytes!",
    "extern crate",
    "pub trait Runner",
];

#[test]
fn renamed_root_fixture_has_an_independent_locked_manifest() {
    let repository = repository_root();
    let fixture = fixture_root(&repository);
    let manifest_source = fs::read_to_string(fixture.join("Cargo.toml"))
        .expect("RPR-U4 requires the external renamed-root fixture manifest");
    let manifest = toml::from_str::<TomlValue>(&manifest_source)
        .expect("renamed-root fixture manifest must be valid TOML");

    assert_eq!(manifest["package"]["name"].as_str(), Some(FIXTURE_PACKAGE));
    assert_eq!(manifest["workspace"]["resolver"].as_str(), Some("3"));
    assert!(manifest.get("patch").is_none());
    assert!(manifest.get("dev-dependencies").is_none());
    assert!(manifest.get("build-dependencies").is_none());

    let dependencies = manifest["dependencies"]
        .as_table()
        .expect("fixture must declare normal dependencies");
    assert_eq!(dependencies.len(), 1);
    let engine = dependencies["engine"]
        .as_table()
        .expect("renamed root dependency must use table syntax");
    assert_eq!(engine["package"].as_str(), Some("nara"));
    assert_eq!(engine["path"].as_str(), Some("../../../.."));
    assert_eq!(engine["default-features"].as_bool(), Some(false));
    assert_eq!(
        engine["features"],
        TomlValue::Array(vec![
            TomlValue::String("runtime-2d".to_owned()),
            TomlValue::String("serde".to_owned()),
        ])
    );
    assert!(fixture.join("Cargo.lock").is_file());

    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["metadata", "--manifest-path"])
        .arg(fixture.join("Cargo.toml"))
        .args(["--locked", "--no-deps", "--format-version", "1"])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to read renamed-root fixture metadata");
    assert_command_succeeded("renamed-root fixture metadata", &output);

    let metadata: JsonValue =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata must be valid JSON");
    let packages = metadata["packages"]
        .as_array()
        .expect("Cargo metadata packages must be an array");
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0]["name"].as_str(), Some(FIXTURE_PACKAGE));
    assert_eq!(packages[0]["dependencies"].as_array().unwrap().len(), 1);
    let dependency = &packages[0]["dependencies"][0];
    assert_eq!(dependency["name"].as_str(), Some("nara"));
    assert_eq!(dependency["rename"].as_str(), Some("engine"));
    assert!(dependency["source"].is_null());

    let workspace_members = metadata["workspace_members"]
        .as_array()
        .expect("fixture workspace members must be an array");
    assert_eq!(workspace_members, &[packages[0]["id"].clone()]);
    assert_eq!(
        Path::new(metadata["workspace_root"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        fixture.canonicalize().unwrap()
    );
}

#[test]
fn renamed_root_fixture_runs_the_ordinary_product_recipe() {
    let repository = repository_root();
    let output = Command::new(cargo_executable())
        .current_dir(&repository)
        .args(["test", "--manifest-path"])
        .arg(fixture_root(&repository).join("Cargo.toml"))
        .args(["--locked", "--jobs", "1", "--", "--test-threads=1"])
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", repository.join("target"))
        .output()
        .expect("failed to run the external renamed-root fixture");
    assert_command_succeeded("renamed-root ordinary recipe fixture", &output);
}

#[test]
fn renamed_root_fixture_uses_only_the_ordinary_product_surface() {
    let source = fs::read_to_string(fixture_root(&repository_root()).join("src/lib.rs"))
        .expect("renamed-root fixture source must be readable");

    for required in REQUIRED_PRODUCT_SURFACE {
        assert!(
            source.contains(required),
            "the external ordinary path must use {required}"
        );
    }
    for forbidden in FORBIDDEN_PRODUCT_SURFACE {
        assert!(
            !source.contains(forbidden),
            "the external ordinary path must not mention {forbidden}"
        );
    }
}

#[test]
fn ordinary_recipe_guide_documents_the_external_contract() {
    let guide = fs::read_to_string(
        repository_root()
            .join("docs")
            .join("guides")
            .join("rust-product-recipes.md"),
    )
    .expect("RPR-U4 requires the public product recipe guide");

    for required in [
        "ProductRecipe",
        "ProductConfiguration",
        "SchemaContribution",
        "HeadlessRun::<GameOutcome>::from_recipe",
        "DesktopRun::from_recipe",
    ] {
        assert!(guide.contains(required), "guide is missing {required:?}");
    }
}

fn assert_command_succeeded(label: &str, output: &Output) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cargo_executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_root(repository: &Path) -> PathBuf {
    repository.join(FIXTURE_RELATIVE_PATH)
}
