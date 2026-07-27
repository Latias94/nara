use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PACKAGE_SCHEMA: &str = "nara.reference-game.candidate-package-v1";
const PLATFORM: &str = "windows-x86_64";
const SOURCE_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

struct TemporaryDirectory {
    path: PathBuf,
}

impl TemporaryDirectory {
    fn new(label: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after the Unix epoch")
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "nara-artifact-package-policy-{label}-{}-{timestamp}-{sequence}",
            std::process::id()
        ));

        fs::create_dir(&path).expect("test temporary directory must be creatable");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let temp_root = env::temp_dir()
            .canonicalize()
            .expect("system temporary directory must resolve");
        let candidate = self
            .path
            .canonicalize()
            .expect("test temporary directory must resolve");
        let repository = repository_root()
            .canonicalize()
            .expect("repository root must resolve");

        assert!(
            candidate.starts_with(&temp_root)
                && candidate != temp_root
                && !candidate.starts_with(&repository),
            "test cleanup must remain inside its own system temporary directory: {candidate:?}"
        );
        fs::remove_dir_all(candidate).expect("test temporary directory must be removable");
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn package_script() -> PathBuf {
    repository_root().join("reference-game/tools/package.py")
}

fn smoke_script() -> PathBuf {
    repository_root().join("reference-game/tools/smoke_artifact.py")
}

fn mutation_script() -> PathBuf {
    repository_root().join("tests/support/artifact_package_fixture.py")
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .expect("test command must start successfully")
}

fn run_package(
    repository: &Path,
    headless: &Path,
    desktop: &Path,
    desktop_probe: &Path,
    output: &Path,
) -> Output {
    run_package_with_receipt(repository, headless, desktop, desktop_probe, output, None)
}

fn run_package_with_receipt(
    repository: &Path,
    headless: &Path,
    desktop: &Path,
    desktop_probe: &Path,
    output: &Path,
    receipt: Option<&Path>,
) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(package_script()).args([
        "create",
        "--repository-root",
        repository.to_str().expect("fixture path must be UTF-8"),
        "--platform",
        PLATFORM,
        "--version",
        "0.1.0",
        "--source-revision",
        SOURCE_REVISION,
        "--headless-binary",
        headless.to_str().expect("fixture path must be UTF-8"),
        "--desktop-binary",
        desktop.to_str().expect("fixture path must be UTF-8"),
        "--desktop-probe-binary",
        desktop_probe.to_str().expect("fixture path must be UTF-8"),
        "--output",
        output.to_str().expect("fixture path must be UTF-8"),
    ]);
    if let Some(receipt) = receipt {
        command
            .arg("--receipt")
            .arg(receipt.to_str().expect("fixture path must be UTF-8"));
    }
    run(&mut command)
}

fn run_bundle(repository: &Path, archive: &Path, receipt: &Path, output: &Path) -> Output {
    run(Command::new("python")
        .arg("-B")
        .arg(package_script())
        .args([
            "bundle",
            "--repository-root",
            repository.to_str().expect("fixture path must be UTF-8"),
            "--archive",
            archive.to_str().expect("fixture path must be UTF-8"),
            "--receipt",
            receipt.to_str().expect("fixture path must be UTF-8"),
            "--output",
            output.to_str().expect("fixture path must be UTF-8"),
        ]))
}

fn run_smoke(arguments: &[&Path]) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(smoke_script());
    for argument in arguments {
        command.arg(argument);
    }
    run(&mut command)
}

fn mutate_archive(mode: &str, source: &Path, output: &Path) {
    let result = run(Command::new("python")
        .arg("-B")
        .arg(mutation_script())
        .arg(mode)
        .arg(source)
        .arg(output));
    assert!(
        result.status.success(),
        "archive mutation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn write_fixture_file(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("fixture file must have a parent"))
        .expect("fixture parent must be creatable");
    fs::write(path, contents).expect("fixture source file must be writable");
}

fn create_repository_fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("fixture repository must be creatable");

    for relative in [
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "reference-game/README.md",
        "reference-game/CONTROLS.md",
        "reference-game/nara.toml",
        "reference-game/assets/kenney-tiny-dungeon.LICENSE.txt",
        "reference-game/assets/textures/player.png",
        "reference-game/assets/textures/player.png.meta",
        "reference-game/assets/textures/tiny-dungeon.png",
        "reference-game/assets/textures/tiny-dungeon.png.meta",
        "reference-game/prefabs/enemy.prefab.json",
        "reference-game/scenes/startup.scene.json",
        "reference-game/schema/component-schema-v1.json",
        "reference-game/schema/component-schema-v2.json",
        "reference-game/schema/component-schema-v3.json",
    ] {
        write_fixture_file(&repository, relative, relative.as_bytes());
    }

    let binaries = root.join("binaries");
    fs::create_dir(&binaries).expect("fixture binary directory must be creatable");
    let headless = binaries.join("headless.exe");
    let desktop = binaries.join("desktop.exe");
    let desktop_probe = binaries.join("desktop_render_probe.exe");
    fs::write(&headless, b"headless binary").expect("headless fixture must be writable");
    fs::write(&desktop, b"desktop binary").expect("desktop fixture must be writable");
    fs::write(&desktop_probe, b"desktop probe binary")
        .expect("desktop probe fixture must be writable");

    (repository, headless, desktop, desktop_probe)
}

#[test]
fn package_creates_a_fixed_checkout_free_payload() {
    let temporary = TemporaryDirectory::new("valid-package");
    let (repository, headless, desktop, desktop_probe) =
        create_repository_fixture(temporary.path());
    let archive = temporary.path().join("candidate.zip");
    let package_receipt = temporary.path().join("candidate-receipt.json");

    let packaged = run_package_with_receipt(
        &repository,
        &headless,
        &desktop,
        &desktop_probe,
        &archive,
        Some(&package_receipt),
    );

    assert!(
        packaged.status.success(),
        "package creation failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );
    assert!(archive.is_file(), "package creation must write one archive");
    assert!(
        package_receipt.is_file(),
        "package creation must write its requested receipt"
    );

    let verify = run_smoke(&[
        Path::new("verify"),
        Path::new("--archive"),
        &archive,
        Path::new("--expected-platform"),
        Path::new(PLATFORM),
        Path::new("--expected-source-revision"),
        Path::new(SOURCE_REVISION),
    ]);
    assert!(
        verify.status.success(),
        "archive preflight failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let receipt: Value = serde_json::from_slice(&verify.stdout)
        .expect("archive preflight must emit one machine-readable receipt");
    assert_eq!(receipt["schema"], PACKAGE_SCHEMA);
    assert_eq!(receipt["platform"], PLATFORM);
    assert_eq!(receipt["source_revision"], SOURCE_REVISION);
    assert!(
        !serde_json::to_string(&receipt)
            .expect("receipt must be serializable")
            .contains(&repository.to_string_lossy().to_string()),
        "consumer receipt must not disclose source checkout paths"
    );

    let consumer_root = temporary.path().join("consumer-root");
    let extract = run_smoke(&[
        Path::new("extract"),
        Path::new("--archive"),
        &archive,
        Path::new("--destination"),
        &consumer_root,
    ]);
    assert!(
        extract.status.success(),
        "archive extraction failed: {}",
        String::from_utf8_lossy(&extract.stderr)
    );
    let package_root = consumer_root.join("nara-reference-game");
    assert!(package_root.join("bin/headless.exe").is_file());
    assert!(package_root.join("bin/desktop.exe").is_file());
    assert!(
        package_root
            .join("tools/desktop-render-probe.exe")
            .is_file()
    );
    assert!(package_root.join("project/nara.toml").is_file());
    assert!(package_root.join("README.md").is_file());
    assert!(package_root.join("LICENSE-MIT").is_file());
    assert!(package_root.join("LICENSE-APACHE").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = fs::metadata(package_root.join("bin/headless.exe"))
            .expect("extracted headless binary must have metadata")
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "extracted binaries must be executable");
    }
    assert!(
        !package_root.join("Cargo.toml").exists(),
        "the extracted candidate must not carry a source checkout"
    );

    let bundle = temporary.path().join("candidate-bundle");
    let bundled = run_bundle(&repository, &archive, &package_receipt, &bundle);
    assert!(
        bundled.status.success(),
        "candidate transport creation failed: {}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    assert!(
        bundle
            .join("verification/reference-game/tools/smoke_artifact.py")
            .is_file()
    );
    assert!(
        bundle
            .join("verification/reference-game/packaging/package-layout-v1.json")
            .is_file()
    );
    let bundled_archive = bundle.join("candidate/candidate.zip");
    assert!(bundled_archive.is_file());
    let bundle_verify = run_smoke(&[
        Path::new("bundle-verify"),
        Path::new("--bundle"),
        &bundle,
        Path::new("--expected-platform"),
        Path::new(PLATFORM),
        Path::new("--expected-source-revision"),
        Path::new(SOURCE_REVISION),
    ]);
    assert!(
        bundle_verify.status.success(),
        "candidate transport verification failed: {}",
        String::from_utf8_lossy(&bundle_verify.stderr)
    );
}

#[test]
fn consumer_rejects_transport_provenance_or_file_tampering() {
    let temporary = TemporaryDirectory::new("tampered-transport");
    let (repository, headless, desktop, desktop_probe) =
        create_repository_fixture(temporary.path());
    let archive = temporary.path().join("candidate.zip");
    let receipt = temporary.path().join("candidate-receipt.json");
    let package = run_package_with_receipt(
        &repository,
        &headless,
        &desktop,
        &desktop_probe,
        &archive,
        Some(&receipt),
    );
    assert!(
        package.status.success(),
        "valid package creation failed: {}",
        String::from_utf8_lossy(&package.stderr)
    );
    let bundle = temporary.path().join("candidate-bundle");
    let bundled = run_bundle(&repository, &archive, &receipt, &bundle);
    assert!(
        bundled.status.success(),
        "valid transport creation failed: {}",
        String::from_utf8_lossy(&bundled.stderr)
    );

    let bundle_manifest = bundle.join("bundle-manifest.json");
    let original_manifest = fs::read(&bundle_manifest).expect("bundle manifest must be readable");
    let mut decoded: Value =
        serde_json::from_slice(&original_manifest).expect("bundle manifest must be valid JSON");
    decoded["source_revision"] =
        Value::String("fedcba9876543210fedcba9876543210fedcba98".to_owned());
    fs::write(
        &bundle_manifest,
        serde_json::to_vec(&decoded).expect("mutated manifest must encode"),
    )
    .expect("mutated manifest must be writable");
    let provenance_rejection = run_smoke(&[
        Path::new("bundle-verify"),
        Path::new("--bundle"),
        &bundle,
        Path::new("--expected-platform"),
        Path::new(PLATFORM),
        Path::new("--expected-source-revision"),
        Path::new(SOURCE_REVISION),
    ]);
    assert!(!provenance_rejection.status.success());
    assert!(
        String::from_utf8_lossy(&provenance_rejection.stderr).contains("revision does not match"),
        "unexpected provenance rejection: {}",
        String::from_utf8_lossy(&provenance_rejection.stderr)
    );

    fs::write(&bundle_manifest, original_manifest).expect("bundle manifest must be restorable");
    let helper = bundle.join("verification/reference-game/tools/package.py");
    let mut helper_contents = fs::read(&helper).expect("verification helper must be readable");
    helper_contents.extend_from_slice(b"\n# transport tamper\n");
    fs::write(&helper, helper_contents).expect("verification helper must be writable");
    let digest_rejection = run_smoke(&[
        Path::new("bundle-verify"),
        Path::new("--bundle"),
        &bundle,
        Path::new("--expected-platform"),
        Path::new(PLATFORM),
        Path::new("--expected-source-revision"),
        Path::new(SOURCE_REVISION),
    ]);
    assert!(!digest_rejection.status.success());
    assert!(
        String::from_utf8_lossy(&digest_rejection.stderr).contains("does not match its manifest"),
        "unexpected digest rejection: {}",
        String::from_utf8_lossy(&digest_rejection.stderr)
    );
}

#[test]
fn package_rejects_output_inside_the_source_checkout() {
    let temporary = TemporaryDirectory::new("output-inside-source");
    let (repository, headless, desktop, desktop_probe) =
        create_repository_fixture(temporary.path());
    let archive = repository.join("candidate.zip");

    let packaged = run_package(&repository, &headless, &desktop, &desktop_probe, &archive);

    assert!(!packaged.status.success());
    assert!(
        String::from_utf8_lossy(&packaged.stderr).contains("outside the repository root"),
        "unexpected rejection: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );
    assert!(!archive.exists(), "rejected output must not be created");
}

#[test]
fn package_rejects_non_regular_binary_inputs_before_writing_an_archive() {
    let temporary = TemporaryDirectory::new("non-regular-binary");
    let (repository, _headless, desktop, desktop_probe) =
        create_repository_fixture(temporary.path());
    let headless_directory = temporary.path().join("not-a-binary");
    fs::create_dir(&headless_directory).expect("fixture directory must be creatable");
    let archive = temporary.path().join("candidate.zip");

    let packaged = run_package(
        &repository,
        &headless_directory,
        &desktop,
        &desktop_probe,
        &archive,
    );

    assert!(!packaged.status.success());
    assert!(
        String::from_utf8_lossy(&packaged.stderr).contains("headless binary"),
        "unexpected rejection: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );
    assert!(
        !archive.exists(),
        "failed validation must not leave an archive"
    );
}

#[test]
fn consumer_rejects_unsafe_or_manifest_inconsistent_archives_before_extraction() {
    let temporary = TemporaryDirectory::new("adversarial-archives");
    let (repository, headless, desktop, desktop_probe) =
        create_repository_fixture(temporary.path());
    let valid_archive = temporary.path().join("valid.zip");
    let packaged = run_package(
        &repository,
        &headless,
        &desktop,
        &desktop_probe,
        &valid_archive,
    );
    assert!(
        packaged.status.success(),
        "valid package creation failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );

    for (index, (mode, expected_error)) in [
        ("unexpected-entry", "missing or unexpected entries"),
        ("traversal", "escapes the fixed package root"),
        ("case-collision", "duplicate or case-colliding entries"),
        ("digest-mismatch", "digest does not match the manifest"),
        ("special-mode", "link or special entry"),
        ("missing-entry", "missing or unexpected entries"),
        ("path-alias", "fixed package root"),
    ]
    .into_iter()
    .enumerate()
    {
        let hostile_archive = temporary.path().join(format!("hostile-{index}.zip"));
        mutate_archive(mode, &valid_archive, &hostile_archive);
        let destination = temporary.path().join(format!("consumer-{index}"));

        let rejected = run_smoke(&[
            Path::new("extract"),
            Path::new("--archive"),
            &hostile_archive,
            Path::new("--destination"),
            &destination,
        ]);

        assert!(!rejected.status.success(), "{mode} archive was accepted");
        assert!(
            String::from_utf8_lossy(&rejected.stderr).contains(expected_error),
            "{mode} produced an unexpected rejection: {}",
            String::from_utf8_lossy(&rejected.stderr)
        );
        assert!(
            !destination.exists(),
            "{mode} must be rejected before accepting a consumer root"
        );
    }
}

#[test]
fn smoke_runner_enforces_output_and_time_limits_while_the_process_is_live() {
    let smoke_script = smoke_script();
    let tool_directory = smoke_script
        .parent()
        .expect("smoke script must have a parent");
    let script = r#"
import os
from pathlib import Path
import sys
import tempfile

sys.path.insert(0, sys.argv[1])
import smoke_artifact as smoke

with tempfile.TemporaryDirectory(prefix="nara-smoke-runner-policy-") as temporary:
    cwd = Path(temporary)
    environment = os.environ.copy()
    isolated_root = cwd / "isolated"
    isolated_root.mkdir()
    isolated_environment, isolated_cwd = smoke.smoke_environment(isolated_root, {})
    assert isolated_cwd == isolated_root / "random-cwd"
    assert "CARGO_HOME" not in isolated_environment
    assert ".cargo" not in isolated_environment["PATH"].lower()

    try:
        smoke.run_bounded(
            [sys.executable, "-c", "import sys; sys.stdout.buffer.write(b'x' * 70000)"],
            cwd,
            environment,
            "output fixture",
        )
    except smoke.ArtifactError as error:
        assert "output byte limit" in str(error), str(error)
    else:
        raise AssertionError("output fixture exceeded the limit without rejection")

    smoke.MAX_PROCESS_TIMEOUT_SECONDS = 0.05
    try:
        smoke.run_bounded(
            [sys.executable, "-c", "import time; time.sleep(2)"],
            cwd,
            environment,
            "timeout fixture",
        )
    except smoke.ArtifactError as error:
        assert "execution time limit" in str(error), str(error)
    else:
        raise AssertionError("timeout fixture exceeded the limit without rejection")
"#;
    let result = run(Command::new("python").arg("-B").arg("-c").arg(script).arg(
        tool_directory
            .to_str()
            .expect("tool directory must be UTF-8"),
    ));

    assert!(
        result.status.success(),
        "bounded smoke runner contract failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
