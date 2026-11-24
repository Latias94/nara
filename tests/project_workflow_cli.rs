#![cfg(all(
    feature = "project-workflow-cli",
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "builds the independent project's headless tooling executable"]
fn compiled_project_check_uses_custom_schema_without_starting_gameplay() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let unrelated = TestRoot::new("compiled-project-check");
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(unrelated.path())
        .arg("check")
        .arg(checkout.join("reference-game"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("NARA_REFERENCE_GAME_STARTUP_MARKER", "1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "compiled project check failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Project check passed"));
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("headless_first_authoritative_tick"),
        "strict Check must not start gameplay",
    );
}

#[test]
#[ignore = "builds the generated project's desktop and headless tool variants"]
fn compiled_project_tools_follow_selected_profile_capabilities() {
    use nara::{
        input::{ActionMapBuilder, ActionMapFileCandidate, ActionMapLimits, InputBinding, KeyCode},
        project::{ProjectManifest, ProjectPath, ProjectProfileOverlay},
    };

    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = checkout.join("target");
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(target.join(format!(
        "nara-project-workflow-cli-profiles-{}-{sequence}",
        std::process::id()
    )));
    let nara = |directory: &Path| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nara"));
        command
            .current_dir(directory)
            .env("CARGO_TARGET_DIR", &target)
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_NET_OFFLINE", "true");
        command
    };
    let created = nara(parent.path())
        .args(["new", "desktop-2d/v2", "profile-selection", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&created.stdout),
        String::from_utf8_lossy(&created.stderr)
    );

    let project = parent.path().join("profile-selection");
    let manifest_path = project.join("nara.toml");
    let mut manifest = ProjectManifest::parse_toml_bytes(&fs::read(&manifest_path).unwrap())
        .manifest
        .unwrap();
    let desktop = manifest.profiles.remove("desktop").unwrap();
    manifest.profiles.insert("qa".to_owned(), desktop);
    manifest
        .profiles
        .insert("desktop".to_owned(), ProjectProfileOverlay::default());
    fs::write(&manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();

    let run = |args: &[&str]| nara(&project).args(args).output().unwrap();
    let assert_passed = |args: &[&str], summary: &str| {
        let output = run(args);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains(summary),
            "{args:?}\nstdout={stdout}\nstderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let check_qa = ["check", ".", "--profile", "qa"];
    assert_passed(&check_qa, "Project check passed");
    assert_passed(
        &["schema", ".", "--check", "--profile", "qa"],
        "The declared authoring schema bundle is current.",
    );
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
    let rejected = run(&check_qa);
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert_eq!(rejected.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("project.tool.content-rejected:"),
        "{stderr}"
    );
    assert!(
        stderr.contains("project.content.input-unsupported-keyboard-binding:")
            && stderr.contains(&format!("\n  at {action_map_path}")),
        "{stderr}"
    );
    fs::write(&map_path, original_map).unwrap();

    let replacement = manifest.startup.default_scene.clone().unwrap();
    manifest
        .replace_startup_scene(Some("qa"), ProjectPath::new("missing.scene.json").unwrap())
        .unwrap();
    fs::write(&manifest_path, manifest.to_canonical_toml_bytes().unwrap()).unwrap();
    assert_passed(
        &[
            "repair",
            "startup-scene",
            ".",
            "--profile",
            "qa",
            "--replacement",
            &replacement,
        ],
        &format!("Repaired startup Scene selection to {replacement}."),
    );
    let repaired = ProjectManifest::parse_toml_bytes(&fs::read(&manifest_path).unwrap())
        .manifest
        .unwrap();
    assert_eq!(
        repaired.profiles["qa"].startup.default_scene.as_deref(),
        Some(replacement.as_str())
    );
    assert_eq!(repaired.startup, manifest.startup);
    assert_passed(&check_qa, "Project check passed");

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
    assert_passed(
        &["check", ".", "--profile", "desktop"],
        "Project check passed",
    );
    assert_passed(&["check", "."], "Project check passed");
    let rejected = run(&["schema", ".", "--check", "--profile", "absent"]);
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert_eq!(rejected.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("project.tool.schema-manifest-rejected:"),
        "{stderr}"
    );
}

#[test]
#[ignore = "builds a real legacy headless binary and exercises compiled-tool bootstrap failures"]
fn legacy_tool_entry_and_failed_bootstrap_never_launch_or_publish() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let root = TestRoot::new("legacy-tool-entry");
    fs::create_dir_all(root.path().join("src/bin")).unwrap();
    let manifest = "[package]\nname = \"legacy-tool-entry\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[workspace]\nresolver = \"3\"\n";
    fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    fs::write(root.path().join("src/bin/headless.rs"), "fn main() { std::fs::write(std::env::var_os(\"NARA_TEST_LEGACY_MARKER\").unwrap(), b\"gameplay\").unwrap(); }").unwrap();
    let build = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--manifest-path"])
        .arg(root.path().join("Cargo.toml"))
        .args(["--bin", "headless"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let marker = root.path().join("gameplay.txt");
    let invoke = || {
        Command::new(env!("CARGO_BIN_EXE_nara"))
            .arg("export")
            .arg(root.path())
            .arg("--parent")
            .arg(root.path())
            .args(["--destination", "must-stay-absent"])
            .env("CARGO_TARGET_DIR", checkout.join("target"))
            .env("CARGO_BUILD_JOBS", "1")
            .env("NARA_TEST_LEGACY_MARKER", &marker)
            .output()
            .unwrap()
    };
    for protocol in [None, Some(2), Some(999)] {
        let metadata = protocol.map_or(String::new(), |version| {
            format!("\n[package.metadata.nara]\nproject-tool-protocol = {version}\n")
        });
        fs::write(
            root.path().join("Cargo.toml"),
            format!("{manifest}{metadata}"),
        )
        .unwrap();
        let output = invoke();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("project-tool-protocol"));
        assert!(
            !marker.exists(),
            "the real legacy executable must not start"
        );
        assert!(!root.path().join("must-stay-absent").exists());
    }
    fs::write(
        root.path().join("Cargo.toml"),
        format!("{manifest}\n[package.metadata.nara]\nproject-tool-protocol = 1\n"),
    )
    .unwrap();
    fs::write(
        root.path().join("src/bin/headless.rs"),
        "compile_error!(\"bootstrap fixture failure\"); fn main() {}\n",
    )
    .unwrap();
    let output = invoke();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("the selected Cargo compilation failed or its protocol was rejected"),
        "{stderr}"
    );
    assert!(
        stderr.contains("bootstrap fixture failure"),
        "the structured compiler diagnostic must remain actionable: {stderr}"
    );
    assert!(!marker.exists());
    assert!(!root.path().join("must-stay-absent").exists());
}

#[test]
fn new_command_creates_and_admits_a_local_project_from_an_unrelated_cwd() {
    let root = TestRoot::new("local-siblings");
    let checkout = root.path().join("engine checkout");
    let parent = root.path().join("games with spaces");
    fs::create_dir(&checkout).unwrap();
    fs::create_dir(&parent).unwrap();
    let unrelated = TestRoot::new("unrelated-cwd");

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(unrelated.path())
        .args(["new", "desktop-2d/v1", "cli-created", "--parent"])
        .arg(&parent)
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();

    let project = parent.join("cli-created");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("project creation was rejected: BeforeStaging"),
            "stdout={}\nstderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            stderr.contains(" is unsupported:")
                || stderr.contains(" cannot prove the required invariant:"),
            "unexpected pre-staging rejection: {stderr}"
        );
        assert!(!project.exists());
        assert!(fs::read_dir(&parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".nara-stage-")
        }));
        return;
    }
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Created cli-created from desktop-2d/v1.\n"
    );

    assert!(project.join("nara.toml").is_file());
    assert!(project.join("scenes/startup.scene.json").is_file());
    assert_created_source_identity(&project);
    let cargo_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    let cargo: toml::Value = toml::from_str(&cargo_manifest).unwrap();
    let dependency = cargo["dependencies"]["nara"]["path"].as_str().unwrap();
    assert_eq!(dependency, "../../engine checkout");
    assert_eq!(
        fs::canonicalize(project.join(dependency)).unwrap(),
        fs::canonicalize(&checkout).unwrap()
    );
    assert!(cargo_manifest.contains("[workspace]\nresolver = \"3\""));
}

fn assert_created_source_identity(project: &Path) {
    let bytes = fs::read(project.join("nara.toml")).unwrap();
    let manifest = nara::project::ProjectManifest::parse_toml_bytes(&bytes)
        .manifest
        .unwrap();
    assert_eq!(
        manifest.project.native_document_identity,
        nara::project::NativeDocumentIdentity::StableMetadata
    );
    assert!(!manifest.project.source_universe.as_uuid().is_nil());
    let metadata = nara::asset::AssetMetaIdentityCandidate::decode_json_bytes(
        &fs::read(project.join("scenes/startup.scene.json.meta")).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata.identity().path().as_str(), "startup.scene.json");
    assert_eq!(
        metadata.identity().source_kind(),
        &nara::asset::AssetSourceKind::Scene
    );
    assert!(!metadata.identity().stable_id().as_uuid().is_nil());
}

#[test]
#[cfg(windows)]
fn new_command_rejects_cross_volume_local_creation_before_publication() {
    let checkout = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap();
    let parent = TestRoot::new("cross-volume-parent");
    let parent_path = fs::canonicalize(parent.path()).unwrap();
    if checkout.components().next() == parent_path.components().next() {
        // This host has no second test volume; the same-volume cases run elsewhere.
        return;
    }
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "cross-volume-game", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cross-volume roots are unsupported"));
    assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[test]
fn new_command_accepts_a_valid_immutable_git_source() {
    let root = TestRoot::new("git-source");
    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(root.path())
        .args(["new", "desktop-2d/v1", "git-created", "--parent"])
        .arg(root.path())
        .args([
            "--git",
            "https://github.com/nara-engine/nara.git",
            "--rev",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();

    let project = root.path().join("git-created");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("project creation was rejected: BeforeStaging")
                && (stderr.contains(" is unsupported:")
                    || stderr.contains(" cannot prove the required invariant:")),
            "unexpected Git-source rejection: {stderr}"
        );
        assert!(!project.exists());
        return;
    }

    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Created git-created from desktop-2d/v1.\n"
    );
    let cargo_manifest = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_manifest.contains(
        "git = \"https://github.com/nara-engine/nara.git\", rev = \"0123456789abcdef0123456789abcdef01234567\""
    ));
    assert!(!cargo_manifest.contains("@"));
    assert_created_source_identity(&project);
}

#[test]
fn generated_project_is_an_independent_cargo_product() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(checkout.join(format!(
        "../nara-project-workflow-cli-cargo-{}-{sequence}",
        std::process::id()
    )));
    let command = |program: &str| {
        let mut command = Command::new(program);
        command
            .current_dir(&checkout)
            .env("CARGO_BUILD_JOBS", "1")
            .env("CARGO_NET_OFFLINE", "true")
            .env("CARGO_TARGET_DIR", checkout.join("target"));
        command
    };

    let output = command(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "nara", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    let project = parent.path().join("nara");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest_bytes = fs::read(project.join("nara.toml")).unwrap();
    let manifest = nara::project::ProjectManifest::parse_toml_bytes(&manifest_bytes)
        .manifest
        .unwrap();
    assert_eq!(manifest.project.name, "nara");
    assert_eq!(manifest.project.stable_id.as_deref(), Some("nara"));
    let desktop = manifest.resolve_profile(Some("desktop")).unwrap();
    assert_eq!(desktop.window.title, "nara");
    let package_name = format!("nara-project-{}", manifest.project.source_universe);

    let lock_path = project.join("Cargo.lock");
    assert!(!lock_path.exists());
    let checked = command(env!("CARGO_BIN_EXE_nara"))
        .arg("check")
        .arg(&project)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
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

    let metadata = command(env!("CARGO"))
        .args([
            "metadata",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        metadata.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&metadata.stdout),
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    assert_eq!(metadata["packages"].as_array().unwrap().len(), 1);
    assert_eq!(metadata["packages"][0]["name"], package_name.as_str());
    assert_eq!(
        fs::canonicalize(metadata["workspace_root"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&project).unwrap()
    );
    let targets = metadata["packages"][0]["targets"].as_array().unwrap();
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
            "player",
        ]
    );

    let check = command(env!("CARGO"))
        .args([
            "check",
            "--locked",
            "--all-targets",
            "--all-features",
            "--manifest-path",
        ])
        .arg(project.join("Cargo.toml"))
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn new_command_rejects_mixed_dependency_sources_before_creation() {
    let checkout = TestRoot::new("mixed-source-checkout");
    fs::create_dir(checkout.path().join("projects")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "mixed-source", "--parent"])
        .arg(checkout.path().join("projects"))
        .arg("--local-checkout")
        .arg(checkout.path())
        .args([
            "--git",
            "https://example.invalid/nara.git",
            "--rev",
            "0123456789abcdef0123456789abcdef01234567",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(!checkout.path().join("projects/mixed-source").exists());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("choose exactly one Nara dependency source")
    );
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nara-project-workflow-cli-{label}-{}-{sequence}",
            std::process::id()
        ));
        Self::at(path)
    }

    fn at(path: PathBuf) -> Self {
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
