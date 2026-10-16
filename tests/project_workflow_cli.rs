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

#[path = "support/project_tooling_game.rs"]
mod custom_game;

#[test]
#[ignore = "runs nested Cargo builds and a real custom-schema starter Check/Run/Repair/Export trajectory"]
fn fresh_custom_project_check_builds_before_strict_admission_and_does_not_start_gameplay() {
    use nara::{
        fs::RelativePath,
        project_host::{ingest_project_manifest, resolve_product_recipe},
    };
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(checkout.join(format!(
        "target/nara-custom-project-tooling-{}-{sequence}",
        std::process::id()
    )));
    let created = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "custom-pulse", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let project = parent.path().join("custom-pulse");
    let library = project.join("src/lib.rs");
    let source = fs::read_to_string(&library).unwrap().replace(
        "    nara::ProductRecipe::new()",
        "    custom_recipe().expect(\"the project's contribution is valid\")",
    );
    fs::write(
        &library,
        format!(
            "{source}\n{}",
            include_str!("support/project_tooling_game.rs")
        ),
    )
    .unwrap();
    let root = nara::project_tooling::host_fs::readable_directory(&project).unwrap();
    let manifest = root
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, custom_game::custom_recipe().unwrap()).unwrap();
    fs::write(
        project.join("schema/authoring.bundle.json"),
        plan.schema_validation()
            .authoring_schema_bundle()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut scene: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("scenes/startup.scene.json")).unwrap())
            .unwrap();
    scene["payload"]["entities"].as_array_mut().unwrap().push(serde_json::json!({
        "id":"pulse", "children":[], "components":{"studio.pulse.Pulse":{"version":1,"value":{"type":"map","value":{"value":{"type":"i64","value":41}}}}}
    }));
    scene["payload"]["roots"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!("pulse"));
    fs::write(
        project.join("scenes/startup.scene.json"),
        serde_json::to_vec_pretty(&scene).unwrap(),
    )
    .unwrap();
    let marker = parent.path().join("gameplay.txt");
    assert!(!project.join("Cargo.lock").exists());
    let checked = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("check")
        .arg(&project)
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        checked.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&checked.stdout),
        String::from_utf8_lossy(&checked.stderr)
    );
    assert!(project.join("Cargo.lock").is_file());
    assert!(!marker.exists(), "Check must not execute startup systems");
    let renamed = fs::read_to_string(&library)
        .unwrap()
        .replace("RenamedPulse", "AnotherNativePulseName");
    fs::write(&library, renamed).unwrap();
    let run = Command::new(env!("CARGO"))
        .args(["run", "--locked", "--offline", "--manifest-path"])
        .arg(project.join("Cargo.toml"))
        .args(["--bin", "headless"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), "migrated-pulse=42\n");
    fs::remove_file(&marker).unwrap();
    fs::rename(
        project.join("scenes/startup.scene.json"),
        project.join("scenes/recovered.scene.json"),
    )
    .unwrap();
    let repaired = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["repair", "startup-scene"])
        .arg(&project)
        .args(["--replacement", "recovered.scene.json"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        repaired.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&repaired.stdout),
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(!marker.exists(), "Repair must not execute startup systems");
    let exported = Command::new(env!("CARGO_BIN_EXE_nara"))
        .arg("export")
        .arg(&project)
        .arg("--parent")
        .arg(parent.path())
        .args(["--destination", "custom-candidate"])
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .env("CARGO_BUILD_JOBS", "1")
        .env("NARA_TEST_PULSE_MARKER", &marker)
        .output()
        .unwrap();
    assert!(
        exported.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&exported.stdout),
        String::from_utf8_lossy(&exported.stderr)
    );
    assert!(String::from_utf8_lossy(&exported.stdout).contains("2 exact product observations"));
    assert_eq!(fs::read_to_string(&marker).unwrap(), "migrated-pulse=42\n");
    let candidate = parent.path().join("custom-candidate");
    assert!(
        candidate
            .join("project/scenes/recovered.scene.json")
            .is_file()
    );
    assert!(
        candidate
            .join(format!("bin/headless{}", std::env::consts::EXE_SUFFIX))
            .is_file()
    );
    assert!(
        candidate
            .join(format!("bin/desktop{}", std::env::consts::EXE_SUFFIX))
            .is_file()
    );
}

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
    for protocol in [None, Some(999)] {
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
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("bootstrap build failed"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("bootstrap fixture failure"),
        "the structured compiler diagnostic must remain actionable"
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
}

#[test]
fn generated_project_is_an_independent_cargo_product() {
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
    let parent = TestRoot::at(checkout.join(format!(
        "../nara-project-workflow-cli-cargo-{}-{sequence}",
        std::process::id()
    )));

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .args(["new", "desktop-2d/v1", "cargo-product", "--parent"])
        .arg(parent.path())
        .arg("--local-checkout")
        .arg(&checkout)
        .output()
        .unwrap();
    let project = parent.path().join("cargo-product");
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata = Command::new(env!("CARGO"))
        .current_dir(&checkout)
        .args([
            "metadata",
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
    assert_eq!(metadata["packages"][0]["name"], "cargo-product");
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
    assert_eq!(target_names, ["desktop", "headless", "nara_game"]);

    let check = Command::new(env!("CARGO"))
        .current_dir(&checkout)
        .env("CARGO_BUILD_JOBS", "1")
        .env("CARGO_TARGET_DIR", checkout.join("target"))
        .args([
            "check",
            "--offline",
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
