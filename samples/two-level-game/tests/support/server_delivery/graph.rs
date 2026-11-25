use std::{collections::BTreeSet, num::NonZeroUsize, path::Path, time::Duration};

use nara::{
    project_tooling::{cargo_build::CargoBuildTools, process::SupervisedProcessCommand},
    project_workflow::cargo::{CargoBuildInvocation, CargoBuildSelection},
};

use crate::cooked_delivery::native::Custody;

const REQUIRED_RUNTIME_PACKAGES: &[&str] =
    &["nara_asset", "nara_physics2d", "nara_scene", "rapier2d"];

const FORBIDDEN_PACKAGES: &[&str] = &[
    "nara_asset_watch",
    "nara_hierarchy",
    "nara_image",
    "nara_input",
    "nara_material",
    "nara_project",
    "nara_render",
    "nara_sprite",
    "nara_tilemap",
    "nara_tooling",
    "nara_transform",
    "nara_sprite_render",
    "nara_sprite_animation",
    "nara_render_wgpu",
    "nara_window",
    "notify",
    "raw-window-handle",
    "winit",
    "egui",
    "gpui",
    "imgui",
    "cpal",
    "rtrb",
    "ctrlc",
    "gilrs",
    "gilrs-core",
    "hound",
];
const FORBIDDEN_PREFIXES: &[&str] = &[
    "wgpu",
    "nara_ui",
    "nara_editor",
    "nara-editor",
    "dear-imgui",
];

pub(super) fn audit(
    custody: &mut Custody,
    tools: &CargoBuildTools,
    target_dir: &Path,
    selection: &CargoBuildSelection,
) -> (SupervisedProcessCommand, String) {
    let project = &custody.fixture().project;
    let manifest = project.join("Cargo.toml");
    let binary_source = project.join("src/bin/server.rs");
    let features = selection.features().join(",");
    let command = SupervisedProcessCommand::new(tools.cargo_path(), project)
        .environment("RUSTC", tools.rustc_path())
        .environment("CARGO_TARGET_DIR", target_dir)
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .bounded_stdout(NonZeroUsize::new(256 * 1024).unwrap());
    let declarations = command.clone().arguments([
        "metadata",
        "--no-deps",
        "--format-version",
        "1",
        "--filter-platform",
        selection.target_triple(),
    ]);
    let tree = command.arguments([
        "tree",
        "--package",
        selection.package(),
        "--target",
        selection.target_triple(),
        "--edges",
        "normal,build",
        "--prefix",
        "none",
        "--format",
        "{p}|{f}",
    ]);
    let locked = |command: SupervisedProcessCommand| {
        command
            .arguments([
                "--locked",
                "--offline",
                "--no-default-features",
                "--features",
                &features,
                "--color",
                "never",
                "--manifest-path",
            ])
            .argument(&manifest)
    };
    let metadata = read(custody, &locked(declarations));
    let invocation = CargoBuildInvocation::from_metadata_declarations(
        metadata.as_bytes(),
        &manifest,
        selection.clone(),
    )
    .expect("Cargo must declare the exact shipped server binary");
    assert_eq!(
        invocation.source_path().canonicalize().unwrap(),
        binary_source.canonicalize().unwrap()
    );
    assert_eq!(
        invocation.target_directory().canonicalize().unwrap(),
        target_dir
    );

    let tree = locked(tree);
    let graph = read(custody, &tree);
    let mut packages = BTreeSet::new();
    for line in graph.lines().filter(|line| !line.is_empty()) {
        let (package, features) = line.split_once('|').expect("Cargo tree row with {p}|{f}");
        let package = package
            .split_whitespace()
            .next()
            .expect("Cargo package name");
        let features = features
            .trim_end_matches(" (*)")
            .split(',')
            .filter(|feature| !feature.is_empty())
            .collect::<BTreeSet<_>>();
        packages.insert(package);
        assert!(
            !FORBIDDEN_PACKAGES.contains(&package)
                && !FORBIDDEN_PREFIXES
                    .iter()
                    .any(|&prefix| package.starts_with(prefix)),
            "server target inherited a forbidden normal/build dependency: {line}"
        );
        if package == invocation.package_name() {
            let requested = invocation
                .features()
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();
            assert_eq!(features, requested, "{line}");
        }
        if package == "nara" {
            assert_eq!(
                features,
                BTreeSet::from(["project-runtime", "runtime-core", "serde"]),
                "{line}"
            );
        }
    }
    for required in [invocation.package_name(), "nara"]
        .into_iter()
        .chain(REQUIRED_RUNTIME_PACKAGES.iter().copied())
    {
        assert!(
            packages.contains(required),
            "server dependency audit missed {required}\n{graph}"
        );
    }
    println!(
        concat!(
            "TWO_LEVEL_SERVER_CARGO_GRAPH package={} bin={} profile={} target={} ",
            "locked=true default_features=false features={} edges=normal,build packages={} ",
            "required_runtime_packages={}"
        ),
        invocation.package_name(),
        invocation.target_name(),
        invocation.profile(),
        invocation.target_triple(),
        features,
        packages.len(),
        REQUIRED_RUNTIME_PACKAGES.join(",")
    );
    (tree, graph)
}

pub(super) fn read(custody: &mut Custody, command: &SupervisedProcessCommand) -> String {
    let output = custody.tool(command, Duration::from_secs(60));
    assert_eq!(output.receipt.exit().code(), Some(0), "{}", output.stderr);
    output.stdout
}
