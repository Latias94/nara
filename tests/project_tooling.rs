#![cfg(all(
    feature = "project-tooling",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use nara::{
    input::{ActionMapBuilder, ActionMapFileCandidate, ActionMapLimits, InputBinding, KeyCode},
    project::{ProjectManifest, ProjectPath, ProjectProfileOverlay},
    project_tooling::{
        ProjectToolFailure, ProjectToolInvocationError, ProjectToolRequest, ProjectToolSummary,
        cargo_build::CargoBuildFailure, invoke_project_tool,
    },
};

#[path = "support/authored_project.rs"]
mod authored_project;
use authored_project::{AuthoredFixture, create_authored_project};

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "builds the independent project's headless tooling executable"]
fn compiled_project_check_uses_custom_schema_without_starting_gameplay() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Gameplay writes to stdout. Only the compiled tool's structured response passes Check.
    check_project(&checkout.join("reference-game"), None).unwrap();
}

#[test]
#[ignore = "builds the generated project's desktop and headless tool variants"]
fn compiled_project_tools_follow_selected_profile_capabilities() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let parent = TestRoot::new(&checkout.join("target"), "profiles");
    let project = create_authored_project(
        checkout,
        parent.path(),
        "profile-selection",
        AuthoredFixture::Actions,
    );
    let manifest_path = project.join("nara.toml");
    let mut manifest = read_manifest(&project);
    let desktop = manifest.profiles.remove("desktop").unwrap();
    manifest.profiles.insert("qa".to_owned(), desktop);
    manifest
        .profiles
        .insert("desktop".to_owned(), ProjectProfileOverlay::default());
    fs::write(&manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();

    check_project(&project, Some("qa")).unwrap();
    let schema = invoke_project_tool(ProjectToolRequest::Schema {
        project: project.clone(),
        profile: Some("qa".to_owned()),
        write: false,
    })
    .unwrap();
    assert_eq!(schema, ProjectToolSummary::Schema { written: false });

    let action_map_path = manifest.input.action_map.as_deref().unwrap();
    let map_path = project.join(action_map_path);
    let original_map = fs::read(&map_path).unwrap();
    let map = ActionMapFileCandidate::decode_json_bytes(&original_map).unwrap();
    let mut bindings = map.definition().bindings().to_vec();
    let keyboard = bindings
        .iter_mut()
        .find(|binding| binding.input == InputBinding::Key(KeyCode::Character('d')))
        .expect("the starter must have a keyboard binding for moving right");
    keyboard.input = InputBinding::Key(KeyCode::Character('é'));
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    for context in map.definition().contexts() {
        builder
            .declare_context(context.context.clone(), context.enabled)
            .unwrap();
    }
    for binding in bindings {
        builder.bind(binding).unwrap();
    }
    let unsupported_map = ActionMapFileCandidate::new(map.product(), builder.compile().unwrap())
        .unwrap()
        .to_json_string()
        .unwrap();
    fs::write(&map_path, &unsupported_map).unwrap();
    let rejected = project_failure(check_project(&project, Some("qa")).unwrap_err());
    assert_eq!(rejected.code(), "project.tool.content-rejected");
    let diagnostic = rejected
        .diagnostics()
        .iter()
        .find(|entry| entry.code() == "project.content.input-unsupported-keyboard-binding")
        .expect("the renamed desktop profile must reject the unsupported binding");
    assert_eq!(diagnostic.project_path(), Some(action_map_path));
    fs::write(&map_path, original_map).unwrap();

    let replacement = manifest.startup.default_scene.clone().unwrap();
    manifest
        .replace_startup_scene(Some("qa"), ProjectPath::new("missing.scene.json").unwrap())
        .unwrap();
    fs::write(&manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
    let repaired = invoke_project_tool(ProjectToolRequest::RepairStartupScene {
        project: project.clone(),
        profile: Some("qa".to_owned()),
        replacement: replacement.clone(),
    })
    .unwrap();
    assert_eq!(
        repaired,
        ProjectToolSummary::Repaired {
            replacement: replacement.clone()
        }
    );
    let repaired = read_manifest(&project);
    assert_eq!(
        repaired.profiles["qa"].startup.default_scene.as_deref(),
        Some(replacement.as_str())
    );
    assert_eq!(repaired.startup, manifest.startup);
    check_project(&project, Some("qa")).unwrap();

    let headless_path = project.join("src/bin/headless.rs");
    let headless = fs::read_to_string(&headless_path).unwrap();
    fs::write(
        headless_path,
        format!(
            "{headless}\n#[cfg(feature = \"desktop\")]\n\
             compile_error!(\"headless Check enabled the desktop Cargo feature\");\n"
        ),
    )
    .unwrap();
    fs::write(&map_path, unsupported_map).unwrap();
    check_project(&project, Some("desktop")).unwrap();
    check_project(&project, None).unwrap();
    let rejected = project_failure(
        invoke_project_tool(ProjectToolRequest::Schema {
            project,
            profile: Some("absent".to_owned()),
            write: false,
        })
        .unwrap_err(),
    );
    assert_eq!(rejected.code(), "project.tool.schema-manifest-rejected");
}

#[test]
#[ignore = "builds a real legacy headless binary and exercises compiled-tool bootstrap failures"]
fn legacy_tool_entry_and_failed_bootstrap_never_launch_gameplay() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = TestRoot::new(&std::env::temp_dir(), "legacy-entry");
    let project = root.path();
    fs::create_dir_all(project.join("src/bin")).unwrap();
    let manifest = "[package]\nname = \"legacy-tool-entry\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\nresolver = \"3\"\n";
    fs::write(project.join("Cargo.toml"), manifest).unwrap();
    fs::write(
        project.join("src/bin/headless.rs"),
        "fn main() { std::fs::write(\"gameplay.txt\", b\"gameplay\").unwrap(); }",
    )
    .unwrap();
    assert_cargo_success(
        fixture_cargo(checkout)
            .args(["build", "--offline", "--manifest-path"])
            .arg(project.join("Cargo.toml"))
            .args(["--bin", "headless"])
            .output()
            .unwrap(),
    );

    let invoke = || {
        invoke_project_tool(ProjectToolRequest::Check {
            project: project.to_path_buf(),
            profile: None,
        })
    };
    for protocol in [None, Some(2), Some(999)] {
        let metadata = protocol.map_or(String::new(), |version| {
            format!("\n[package.metadata.nara]\nproject-tool-protocol = {version}\n")
        });
        fs::write(project.join("Cargo.toml"), format!("{manifest}{metadata}")).unwrap();
        let error = invoke().unwrap_err();
        let ProjectToolInvocationError::Bootstrap(message) = error else {
            panic!("expected rejection of the tool protocol declaration: {error:?}");
        };
        assert!(message.contains("project-tool-protocol = 1"), "{message}");
        assert!(!project.join("gameplay.txt").exists());
    }
    fs::write(
        project.join("Cargo.toml"),
        format!("{manifest}\n[package.metadata.nara]\nproject-tool-protocol = 1\n"),
    )
    .unwrap();
    fs::write(
        project.join("src/bin/headless.rs"),
        "compile_error!(\"bootstrap fixture failure\"); fn main() {}\n",
    )
    .unwrap();
    let failure = invoke().unwrap_err();
    let ProjectToolInvocationError::ProcessOwner(CargoBuildFailure::Compilation {
        diagnostics,
        ..
    }) = failure
    else {
        panic!("expected a structured Cargo compilation failure: {failure:?}");
    };
    assert!(
        diagnostics
            .entries()
            .iter()
            .any(|entry| entry.message().contains("bootstrap fixture failure")),
        "{diagnostics:?}"
    );
    assert!(!project.join("gameplay.txt").exists());
}

#[test]
fn authored_fixture_is_an_independent_cargo_product() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let parent = TestRoot::new(checkout.parent().unwrap(), "independent");
    let project = create_authored_project(checkout, parent.path(), "nara", AuthoredFixture::Basic);
    let manifest = read_manifest(&project);
    assert_eq!(manifest.project.name, "nara");
    assert_eq!(manifest.project.stable_id.as_deref(), Some("nara"));
    let desktop = manifest.resolve_profile(Some("desktop")).unwrap();
    assert_eq!(desktop.window.title, "nara");
    let package_name = format!("nara-project-{}", manifest.project.source_universe);

    let lock_path = project.join("Cargo.lock");
    assert!(!lock_path.exists());
    let manifest_path = project.join("Cargo.toml");
    assert_cargo_success(
        fixture_cargo(checkout)
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest_path)
            .output()
            .unwrap(),
    );
    let lock: toml::Value = toml::from_str(&fs::read_to_string(lock_path).unwrap()).unwrap();
    let packages = lock["package"].as_array().unwrap();
    let engine = packages
        .iter()
        .find(|package| package["name"].as_str() == Some("nara"))
        .expect("Cargo.lock must contain the Nara engine");
    assert!(engine.get("source").is_none());
    let game = packages
        .iter()
        .find(|package| package["name"].as_str() == Some(package_name.as_str()))
        .expect("Cargo.lock must contain the generated game");
    assert!(
        game["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|dependency| dependency.as_str() == Some("nara"))
    );

    let metadata = assert_cargo_success(
        fixture_cargo(checkout)
            .args([
                "metadata",
                "--locked",
                "--no-deps",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(&manifest_path)
            .output()
            .unwrap(),
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0]["name"], package_name.as_str());
    assert_eq!(
        fs::canonicalize(metadata["workspace_root"].as_str().unwrap()).unwrap(),
        project,
    );
    let targets = packages[0]["targets"].as_array().unwrap();
    let mut target_names = targets
        .iter()
        .map(|target| target["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    target_names.sort_unstable();
    assert_eq!(
        target_names,
        [
            "build-script-build",
            "cook",
            "desktop",
            "headless",
            "nara_game",
            "player"
        ]
    );

    assert_cargo_success(
        fixture_cargo(checkout)
            .args([
                "check",
                "--locked",
                "--all-targets",
                "--all-features",
                "--manifest-path",
            ])
            .arg(&manifest_path)
            .output()
            .unwrap(),
    );
}

fn check_project(project: &Path, profile: Option<&str>) -> Result<(), ProjectToolInvocationError> {
    let checked = invoke_project_tool(ProjectToolRequest::Check {
        project: project.to_path_buf(),
        profile: profile.map(str::to_owned),
    })?;
    assert!(
        matches!(
            checked,
            ProjectToolSummary::Checked {
                content_members: 1..
            }
        ),
        "{checked:?}"
    );
    Ok(())
}

fn project_failure(error: ProjectToolInvocationError) -> ProjectToolFailure {
    match error {
        ProjectToolInvocationError::Project(failure) => failure,
        error => panic!("expected a compiled project failure: {error:?}"),
    }
}

fn read_manifest(project: &Path) -> ProjectManifest {
    let parsed = ProjectManifest::parse_toml_bytes(&fs::read(project.join("nara.toml")).unwrap());
    assert!(!parsed.has_errors());
    parsed.manifest.unwrap()
}

fn fixture_cargo(checkout: &Path) -> Command {
    let mut cargo = Command::new(env!("CARGO"));
    cargo
        .current_dir(checkout)
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_NET_OFFLINE", "true")
        .env("CARGO_TARGET_DIR", checkout.join("target"));
    cargo
}

fn assert_cargo_success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    output
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(parent: &Path, label: &str) -> Self {
        let parent = parent.canonicalize().unwrap();
        let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "nara-project-tooling-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        if self.path.canonicalize().is_ok_and(|path| path == self.path) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
