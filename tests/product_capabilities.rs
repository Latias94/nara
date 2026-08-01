use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

use nara::{
    ServerPlugins, app::App, input::PointerState, project::ProductCapability,
    project_host::compiled_product_capabilities, window::WindowEvents,
};
use serde_json::Value;

#[test]
fn compiled_ceiling_matches_the_active_cargo_features() {
    let capabilities = compiled_product_capabilities().capabilities();
    let expected = [
        (ProductCapability::RuntimeCore, true),
        (ProductCapability::Runtime2d, cfg!(feature = "runtime-2d")),
        (ProductCapability::RuntimeUi, cfg!(feature = "runtime-ui")),
        (ProductCapability::Tooling, cfg!(feature = "tooling")),
        (ProductCapability::AssetWatch, cfg!(feature = "asset-watch")),
        (
            ProductCapability::DesktopWinit,
            cfg!(feature = "desktop-winit"),
        ),
        (ProductCapability::RenderWgpu, cfg!(feature = "render-wgpu")),
        (
            ProductCapability::ToolingEgui,
            cfg!(feature = "tooling-egui"),
        ),
    ]
    .into_iter()
    .filter_map(|(capability, enabled)| enabled.then_some(capability))
    .collect::<Vec<_>>();

    assert_eq!(capabilities.iter().collect::<Vec<_>>(), expected);
}

#[test]
fn server_bundle_stays_free_of_raw_client_resources() {
    let mut app = App::new();
    app.add_plugins(ServerPlugins).unwrap();

    assert!(!app.world().contains_resource::<PointerState>());
    assert!(!app.world().contains_resource::<WindowEvents>());
    #[cfg(feature = "render-wgpu")]
    assert!(
        !app.world()
            .contains_resource::<nara::render_wgpu::WgpuRenderBackend>()
    );
    #[cfg(feature = "asset-watch")]
    assert!(
        !app.world()
            .contains_resource::<nara::asset_watch::AssetWatcher>()
    );
}

#[test]
fn root_manifest_exposes_only_coarse_product_capabilities() {
    let metadata = cargo_metadata();
    let package = root_package(&metadata);
    let features = package["features"].as_object().unwrap();

    let expected = [
        "asset-watch",
        "default",
        "desktop-winit",
        "render-wgpu",
        "runtime-2d",
        "runtime-core",
        "runtime-ui",
        "serde",
        "tooling",
        "tooling-egui",
    ];
    assert_eq!(
        features.keys().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(feature_members(features, "default"), vec!["runtime-core"]);
    assert_eq!(
        feature_members(features, "serde"),
        [
            "nara_asset?/serde",
            "nara_core?/serde",
            "nara_diagnostic?/serde",
            "nara_ecs?/serde",
            "nara_image?/serde",
            "nara_identity?/serde",
            "nara_input?/serde",
            "nara_gameplay?/serde",
            "nara_material?/serde",
            "nara_reflect?/serde",
            "nara_transform?/serde",
            "nara_render?/serde",
            "nara_scene?/serde",
            "nara_sprite?/serde",
            "nara_tilemap?/serde",
            "nara_ui?/serde",
            "nara_window?/serde",
        ]
    );

    for dependency in package["dependencies"].as_array().unwrap() {
        let name = dependency["name"].as_str().unwrap();
        if name.starts_with("nara_") {
            assert_eq!(
                dependency["optional"].as_bool(),
                Some(true),
                "root engine dependency {name} must be optional"
            );
        }
    }
}

#[test]
fn examples_and_optional_tests_declare_their_capability_ceiling() {
    let metadata = cargo_metadata();
    let package = root_package(&metadata);
    let targets = package["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|target| {
            (
                target["name"].as_str().unwrap(),
                target
                    .get("required-features")
                    .and_then(Value::as_array)
                    .map(|features| {
                        features
                            .iter()
                            .map(|feature| feature.as_str().unwrap())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            )
        })
        .collect::<BTreeMap<_, _>>();

    assert_eq!(targets["windowed_clear"], ["desktop-winit", "render-wgpu"]);
    assert_eq!(
        targets["windowed_sprites"],
        ["runtime-2d", "desktop-winit", "render-wgpu"]
    );
    assert_eq!(
        targets["runtime_ui_panel"],
        ["runtime-ui", "desktop-winit", "render-wgpu"]
    );
    assert_eq!(targets["scene_file_workspace"], ["tooling", "serde"]);
    assert_eq!(targets["product_capabilities"], ["runtime-core"]);
    assert_eq!(targets["project_composition"], ["runtime-core"]);
    assert_eq!(targets["scene_authoring_session"], ["runtime-core"]);
    assert_eq!(targets["scene_inspector"], ["tooling"]);
    assert_eq!(targets["scene_patch_transactions"], ["runtime-core"]);
    for target in [
        "editor_persistence",
        "scene_play_mode",
        "workspace_play_runtime",
    ] {
        assert_eq!(targets[target], ["tooling", "runtime-2d", "serde"]);
    }
    assert_eq!(targets["scene_sprite_serialization"], ["runtime-2d"]);
    assert_eq!(targets["stable_runtime_identity"], ["tooling"]);
}

#[test]
fn wgpu_submitter_dependency_trees_are_isolated() {
    let base = cargo_tree(&["--no-default-features", "--features", "render-wgpu"]);
    assert_not_present(&base, "nara_image");
    assert_not_present(&base, "nara_material");
    assert_not_present(&base, "nara_sprite_render");
    assert_not_present(&base, "nara_ui_render");

    let sprites = cargo_tree(&[
        "--no-default-features",
        "--features",
        "runtime-2d,render-wgpu",
    ]);
    assert_present(&sprites, "nara_sprite_render");
    assert_not_present(&sprites, "nara_ui");
    assert_not_present(&sprites, "nara_ui_render");

    let ui = cargo_tree(&[
        "--no-default-features",
        "--features",
        "runtime-ui,render-wgpu",
    ]);
    assert_present(&ui, "nara_ui_render");
    assert_not_present(&ui, "nara_sprite");
    assert_not_present(&ui, "nara_sprite_render");
    assert_not_present(&ui, "nara_tilemap");
}

#[test]
fn locked_dependency_trees_match_the_coarse_feature_contract() {
    let runtime_core = package_set(&[
        "nara_app",
        "nara_asset",
        "nara_core",
        "nara_diagnostic",
        "nara_ecs",
        "nara_ecs_derive",
        "nara_fs",
        "nara_gameplay",
        "nara_hierarchy",
        "nara_identity",
        "nara_input",
        "nara_project",
        "nara_reflect",
        "nara_reflect_derive",
        "nara_scene",
        "nara_tasks",
        "nara_transform",
        "nara_window",
    ]);
    let cases = [
        (
            "no-default-features",
            vec!["--no-default-features"],
            BTreeSet::new(),
        ),
        ("default", Vec::new(), runtime_core.clone()),
        (
            "serde-only",
            vec!["--no-default-features", "--features", "serde"],
            BTreeSet::new(),
        ),
        (
            "runtime-core",
            vec!["--no-default-features", "--features", "runtime-core"],
            runtime_core.clone(),
        ),
        (
            "runtime-2d",
            vec!["--no-default-features", "--features", "runtime-2d"],
            with_packages(
                &runtime_core,
                &[
                    "nara_image",
                    "nara_material",
                    "nara_render",
                    "nara_sprite",
                    "nara_sprite_render",
                    "nara_tilemap",
                ],
            ),
        ),
        (
            "runtime-ui",
            vec!["--no-default-features", "--features", "runtime-ui"],
            with_packages(
                &runtime_core,
                &[
                    "nara_image",
                    "nara_material",
                    "nara_render",
                    "nara_ui",
                    "nara_ui_render",
                ],
            ),
        ),
        (
            "tooling",
            vec!["--no-default-features", "--features", "tooling"],
            with_packages(&runtime_core, &["nara_tooling"]),
        ),
        (
            "asset-watch",
            vec!["--no-default-features", "--features", "asset-watch"],
            with_packages(&runtime_core, &["nara_asset_watch"]),
        ),
        (
            "desktop-winit",
            vec!["--no-default-features", "--features", "desktop-winit"],
            with_packages(&runtime_core, &["nara_winit"]),
        ),
        (
            "render-wgpu",
            vec!["--no-default-features", "--features", "render-wgpu"],
            with_packages(&runtime_core, &["nara_render", "nara_render_wgpu"]),
        ),
        (
            "tooling-egui",
            vec!["--no-default-features", "--features", "tooling-egui"],
            with_packages(&runtime_core, &["nara_tooling", "nara_tooling_egui"]),
        ),
        (
            "all-features",
            vec!["--all-features"],
            with_packages(
                &runtime_core,
                &[
                    "nara_asset_watch",
                    "nara_image",
                    "nara_material",
                    "nara_render",
                    "nara_render_wgpu",
                    "nara_sprite",
                    "nara_sprite_render",
                    "nara_tilemap",
                    "nara_tooling",
                    "nara_tooling_egui",
                    "nara_ui",
                    "nara_ui_render",
                    "nara_winit",
                ],
            ),
        ),
    ];

    for (name, arguments, expected) in cases {
        let tree = cargo_tree(&arguments);
        assert_eq!(
            internal_nara_packages(&tree),
            expected,
            "unexpected internal package graph for {name}\n{tree}"
        );
    }
}

#[test]
fn public_prelude_is_gameplay_first() {
    check_public_prelude_fixture("gameplay-pass", true, None);
    check_public_prelude_fixture("recipe-pass", true, None);
    check_public_prelude_fixture("one-shot-direct-pass", true, None);
    check_public_prelude_fixture("explicit-surfaces-pass", true, None);
    check_public_prelude_fixture("recipe-one-shot-fail", false, None);
    check_public_prelude_fixture("hierarchy-construction-fail", false, None);
    check_public_prelude_fixture("hierarchy-children-mutation-fail", false, None);
    check_public_prelude_fixture("hierarchy-retirement-path-fail", false, None);
    check_public_prelude_fixture(
        "hierarchy-set-fail",
        false,
        Some(PublicPreludeFailure::Private("HierarchySet")),
    );
    check_public_prelude_fixture(
        "scene-hierarchy-path-fail",
        false,
        Some(PublicPreludeFailure::Missing("Parent")),
    );
    check_public_prelude_fixture(
        "identity-internal-path-fail",
        false,
        Some(PublicPreludeFailure::Missing("__private")),
    );
    check_public_prelude_fixture(
        "identity-prepared-method-fail",
        false,
        Some(PublicPreludeFailure::Private(
            "prepare_exact_scene_instance_replacement",
        )),
    );
    for (binary, symbol) in [
        ("backend-fail", "WindowEvents"),
        ("tooling-fail", "SceneInspectorState"),
        ("diagnostic-storage-fail", "RuntimeDiagnostics"),
        ("queue-lifecycle-fail", "GameplayCommandQueue"),
        ("project-host-fail", "ProjectSettingsCandidate"),
    ] {
        check_public_prelude_fixture(binary, false, Some(PublicPreludeFailure::Missing(symbol)));
    }
}

#[derive(Clone, Copy)]
enum PublicPreludeFailure<'a> {
    Missing(&'a str),
    Private(&'a str),
}

fn cargo_metadata() -> Value {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--locked", "--format-version=1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn root_package(metadata: &Value) -> &Value {
    metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "nara")
        .unwrap()
}

fn feature_members<'a>(features: &'a serde_json::Map<String, Value>, name: &str) -> Vec<&'a str> {
    features[name]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member.as_str().unwrap())
        .collect()
}

fn cargo_tree(arguments: &[&str]) -> String {
    let mut command = Command::new(env!("CARGO"));
    command.args([
        "tree", "-p", "nara", "-e", "normal", "--locked", "--prefix", "none",
    ]);
    let output = command
        .args(arguments)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn package_set(packages: &[&'static str]) -> BTreeSet<&'static str> {
    packages.iter().copied().collect()
}

fn with_packages(
    base: &BTreeSet<&'static str>,
    packages: &[&'static str],
) -> BTreeSet<&'static str> {
    base.iter()
        .copied()
        .chain(packages.iter().copied())
        .collect()
}

fn internal_nara_packages(tree: &str) -> BTreeSet<&str> {
    tree.lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|package| package.starts_with("nara_"))
        .collect()
}

fn check_public_prelude_fixture(
    binary: &str,
    should_succeed: bool,
    failure: Option<PublicPreludeFailure<'_>>,
) {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = repository
        .join("tests")
        .join("fixtures")
        .join("public-prelude")
        .join("Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["check", "--manifest-path"])
        .arg(&manifest)
        .args(["--locked", "--jobs", "1", "--bin", binary])
        .current_dir(repository)
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TARGET_DIR", public_prelude_target_dir(repository))
        .output()
        .unwrap_or_else(|error| panic!("failed to check public prelude fixture {binary}: {error}"));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.success(),
        should_succeed,
        "unexpected compile result for public prelude fixture {binary}\nstdout:\n{}\nstderr:\n{stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    if let Some(failure) = failure {
        match failure {
            PublicPreludeFailure::Missing(symbol) => assert!(
                (stderr.contains("unresolved import") || stderr.contains("cannot find type"))
                    && stderr.contains(symbol),
                "{binary} must fail because {symbol} is absent from the ordinary facade\n{stderr}"
            ),
            PublicPreludeFailure::Private(symbol) => assert!(
                (stderr.contains("is private") || stderr.contains("private associated function"))
                    && stderr.contains(symbol),
                "{binary} must fail because {symbol} is private to its owning crate\n{stderr}"
            ),
        }
    }
}

fn public_prelude_target_dir(repository: &Path) -> PathBuf {
    repository.join("target").join("public-prelude-fixtures")
}

fn assert_present(tree: &str, package: &str) {
    assert!(
        tree.lines()
            .any(|line| line.starts_with(&format!("{package} ")))
    );
}

fn assert_not_present(tree: &str, package: &str) {
    assert!(
        !tree
            .lines()
            .any(|line| line.starts_with(&format!("{package} ")))
    );
}
