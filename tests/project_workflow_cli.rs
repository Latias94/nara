#![cfg(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64")
))]

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

#[test]
fn new_command_creates_and_admits_a_local_project_from_an_unrelated_cwd() {
    let checkout = TestRoot::new("local-checkout");
    let unrelated = TestRoot::new("unrelated-cwd");
    fs::create_dir(checkout.path().join("projects")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_nara"))
        .current_dir(unrelated.path())
        .args(["new", "desktop-2d/v1", "cli-created", "--parent"])
        .arg(checkout.path().join("projects"))
        .arg("--local-checkout")
        .arg(checkout.path())
        .output()
        .unwrap();

    let project = checkout.path().join("projects/cli-created");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("project creation was rejected: BeforeStaging"),
            "stdout={}\nstderr={stderr}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(!project.exists());
        assert!(
            fs::read_dir(checkout.path().join("projects"))
                .unwrap()
                .all(|entry| {
                    !entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".nara-stage-")
                })
        );
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
    assert!(cargo_manifest.contains("path = \"../..\""));
    assert!(cargo_manifest.contains("[workspace]\nresolver = \"3\""));
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
