use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

use nara::{
    ServerPlugins, app::App, project::ProductCapability,
    project_host::compiled_product_capabilities,
};
use serde_json::Value;

#[test]
fn compiled_ceiling_matches_the_active_cargo_features() {
    let capabilities = compiled_product_capabilities().capabilities();
    let expected = [
        (ProductCapability::RuntimeCore, true),
        (ProductCapability::Runtime2d, cfg!(feature = "runtime-2d")),
        (ProductCapability::RuntimeUi, cfg!(feature = "runtime-ui")),
        (
            ProductCapability::Presentation2d,
            cfg!(feature = "presentation-2d"),
        ),
        (
            ProductCapability::PresentationUi,
            cfg!(feature = "presentation-ui"),
        ),
        (ProductCapability::AssetWatch, cfg!(feature = "asset-watch")),
        (
            ProductCapability::DesktopWinit,
            cfg!(feature = "desktop-winit"),
        ),
        (ProductCapability::RenderWgpu, cfg!(feature = "render-wgpu")),
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

    #[cfg(feature = "runtime-input")]
    {
        assert!(!app.world().contains_resource::<nara::input::PointerState>());
        assert!(!app.world().contains_resource::<nara::input::ActionState>());
    }
    #[cfg(feature = "runtime-window")]
    assert!(
        !app.world()
            .contains_resource::<nara::window::WindowEvents>()
    );
    #[cfg(feature = "render-wgpu")]
    {
        assert!(
            !app.world()
                .contains_resource::<nara::render_wgpu::execution::WgpuRuntimeConnection>()
        );
        assert!(
            !app.world()
                .contains_resource::<nara::render_wgpu::WgpuRenderObservation>()
        );
    }
    #[cfg(feature = "asset-watch")]
    assert!(
        !app.world()
            .contains_resource::<nara::asset_watch::AssetWatcher>()
    );
}

#[cfg(feature = "render-wgpu")]
#[test]
fn wgpu_bundle_installs_cpu_exchange_without_making_the_gpu_owner_a_resource() {
    static_assertions::assert_not_impl_any!(nara::render_wgpu::WgpuRenderHost: nara::ecs::Resource);

    let mut app = App::new();
    app.add_plugins((nara::MinimalPlugins, nara::WgpuBackendPlugins))
        .unwrap();

    assert!(
        app.world()
            .contains_resource::<nara::render_wgpu::execution::WgpuRuntimeConnection>()
    );
    assert!(
        app.world()
            .contains_resource::<nara::render_wgpu::WgpuRenderObservation>()
    );
}

#[test]
fn root_manifest_exposes_only_coarse_product_capabilities() {
    let metadata = cargo_metadata();
    let package = root_package(&metadata);
    let features = package["features"].as_object().unwrap();

    let expected = [
        "asset-watch",
        "audio",
        "audio-device",
        "default",
        "desktop-winit",
        "gamepad",
        "parallel-executor",
        "physics-2d",
        "presentation-2d",
        "presentation-ui",
        "project-runtime",
        "project-tooling",
        "project-workflow",
        "project-workflow-cargo-facts",
        "render-wgpu",
        "runtime-2d",
        "runtime-core",
        "runtime-input",
        "runtime-ui",
        "runtime-window",
        "scene-work-observation",
        "serde",
    ];
    assert_eq!(
        features.keys().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        feature_members(features, "default"),
        ["runtime-core", "runtime-input", "runtime-window"]
    );
    assert_eq!(
        feature_members(features, "runtime-input"),
        ["runtime-core", "dep:nara_input"]
    );
    assert_eq!(
        feature_members(features, "runtime-window"),
        ["runtime-core", "dep:nara_window"]
    );
    assert_eq!(
        feature_members(features, "runtime-2d"),
        [
            "runtime-input",
            "runtime-window",
            "dep:nara_render",
            "nara_render/image-runtime",
            "nara_render/animation",
            "nara_render/persistent",
            "nara_render/sprites",
            "nara_render/tiles",
        ]
    );
    assert_eq!(
        feature_members(features, "presentation-2d"),
        ["runtime-2d", "nara_render/sprite-render"]
    );
    assert_eq!(
        feature_members(features, "runtime-ui"),
        [
            "runtime-input",
            "runtime-window",
            "dep:nara_ui",
            "nara_ui/runtime"
        ]
    );
    assert_eq!(
        feature_members(features, "presentation-ui"),
        [
            "runtime-ui",
            "dep:nara_render",
            "nara_render/hud-presentation"
        ]
    );
    assert_eq!(
        feature_members(features, "serde"),
        [
            "dep:serde",
            "dep:serde_json",
            "nara_app?/serde",
            "nara_asset?/serde",
            "nara_core?/serde",
            "nara_ecs?/serde",
            "nara_input?/serde",
            "nara_reflect?/serde",
            "nara_render?/serde",
            "nara_scene?/serde",
            "nara_ui?/serde",
            "nara_window?/serde",
        ]
    );

    for dependency in package["dependencies"].as_array().unwrap() {
        let name = dependency["name"].as_str().unwrap();
        if dependency["kind"].is_null() && name.starts_with("nara_") {
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
        [
            "project-runtime",
            "presentation-2d",
            "desktop-winit",
            "render-wgpu"
        ]
    );
    assert_eq!(
        targets["ui_presentation_gallery"],
        [
            "project-runtime",
            "presentation-ui",
            "desktop-winit",
            "render-wgpu"
        ]
    );
    assert_eq!(targets["asset_import_texture"], ["presentation-2d"]);
    assert_eq!(targets["hello_world"], ["runtime-2d"]);
    assert_eq!(targets["sprite_tilemap_scene"], ["presentation-2d"]);
    assert_eq!(
        targets["scene_prefab_roundtrip"],
        ["presentation-2d", "serde"]
    );
    assert_eq!(
        targets["product_capabilities"],
        ["project-runtime", "runtime-core"]
    );
    assert_eq!(
        targets["project_composition"],
        ["project-runtime", "runtime-core"]
    );
    assert_eq!(
        targets["runtime_configuration_composition"],
        ["project-runtime", "runtime-core"]
    );
    assert_eq!(
        targets["runtime_scene_activation"],
        ["project-runtime", "runtime-core", "serde"]
    );
    assert_eq!(targets["scene_authoring_session"], ["runtime-core"]);
    assert_eq!(targets["scene_patch_transactions"], ["runtime-core"]);
    assert_eq!(targets["scene_sprite_serialization"], ["runtime-2d"]);
}

#[test]
fn wgpu_submitter_dependency_trees_are_isolated() {
    for (features, sprites, hud) in [
        ("render-wgpu", false, false),
        ("runtime-2d,render-wgpu", false, false),
        ("presentation-2d,render-wgpu", true, false),
        ("presentation-ui,render-wgpu", false, true),
    ] {
        let tree = cargo_tree(&["--no-default-features", "--features", features]);
        let renderer = tree
            .lines()
            .find(|line| line.starts_with("nara_render "))
            .unwrap();
        let enabled = renderer.split_once('|').unwrap().1.trim_end_matches(" (*)");
        let enabled: BTreeSet<_> = enabled.split(',').collect();
        assert_eq!(enabled.contains("sprite-render"), sprites, "{renderer}");
        assert_eq!(enabled.contains("hud-runtime"), hud, "{renderer}");
        if !hud {
            assert_not_present(&tree, "nara_ui");
        }
    }
}

#[test]
fn locked_dependency_trees_match_the_coarse_feature_contract() {
    let runtime_core = package_set(&[
        "nara_app",
        "nara_asset",
        "nara_core",
        "nara_ecs",
        "nara_ecs_derive",
        "nara_reflect",
        "nara_reflect_derive",
        "nara_scene",
    ]);
    let local_runtime = with_packages(&runtime_core, &["nara_input", "nara_window"]);
    let cases = [
        (
            "no-default-features",
            vec!["--no-default-features"],
            BTreeSet::new(),
        ),
        ("default", Vec::new(), local_runtime.clone()),
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
            "runtime-core-serde",
            vec!["--no-default-features", "--features", "runtime-core,serde"],
            runtime_core.clone(),
        ),
        (
            "runtime-input",
            vec!["--no-default-features", "--features", "runtime-input"],
            with_packages(&runtime_core, &["nara_input"]),
        ),
        (
            "runtime-window",
            vec!["--no-default-features", "--features", "runtime-window"],
            with_packages(&runtime_core, &["nara_window"]),
        ),
        (
            "runtime-2d",
            vec!["--no-default-features", "--features", "runtime-2d"],
            with_packages(&local_runtime, &["nara_material_derive", "nara_render"]),
        ),
        (
            "runtime-ui",
            vec!["--no-default-features", "--features", "runtime-ui"],
            with_packages(&local_runtime, &["nara_ui"]),
        ),
        (
            "presentation-2d",
            vec!["--no-default-features", "--features", "presentation-2d"],
            with_packages(&local_runtime, &["nara_material_derive", "nara_render"]),
        ),
        (
            "presentation-ui",
            vec!["--no-default-features", "--features", "presentation-ui"],
            with_packages(&local_runtime, &["nara_render", "nara_ui"]),
        ),
        (
            "asset-watch",
            vec!["--no-default-features", "--features", "asset-watch"],
            runtime_core.clone(),
        ),
        (
            "desktop-winit",
            vec!["--no-default-features", "--features", "desktop-winit"],
            local_runtime.clone(),
        ),
        (
            "render-wgpu",
            vec!["--no-default-features", "--features", "render-wgpu"],
            with_packages(
                &runtime_core,
                &["nara_material_derive", "nara_render", "nara_window"],
            ),
        ),
        (
            "all-features",
            vec!["--all-features"],
            with_packages(
                &local_runtime,
                &[
                    "nara_audio",
                    "nara_material_derive",
                    "nara_physics2d",
                    "nara_project",
                    "nara_render",
                    "nara_ui",
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
        for tool_package in ["nara_editor", "nara_editor_core", "gpui", "ctrlc"] {
            assert_not_present(&tree, tool_package);
        }
        if !matches!(name, "desktop-winit" | "render-wgpu" | "all-features") {
            assert_not_present(&tree, "winit");
            assert_not_present(&tree, "wgpu");
        }
    }
}

#[test]
fn game_ui_adapter_dependency_closure_excludes_editor_frontends() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("crates")
        .join("nara_ui")
        .join("Cargo.toml");
    let tree = cargo_tree_for_manifest(
        &manifest,
        "nara_ui",
        &["-e", "normal,build", "--target", "all"],
    );

    assert_present(&tree, "nara_ui");
    for package in ["nara_editor", "nara_editor_core", "gpui"] {
        assert_not_present(&tree, package);
    }
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
        "tree", "-p", "nara", "-e", "normal", "--locked", "--prefix", "none", "--format", "{p}|{f}",
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

fn cargo_tree_for_manifest(manifest: &Path, package: &str, arguments: &[&str]) -> String {
    let output = Command::new(env!("CARGO"))
        .args(["tree", "--manifest-path"])
        .arg(manifest)
        .args(["-p", package, "--locked", "--prefix", "none"])
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
