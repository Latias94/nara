use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PACKAGE_SCHEMA: &str = "nara.reference-game.candidate-package-v2";
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

fn run_smoke_inline(script: &str) -> Output {
    let smoke = smoke_script();
    let tool_directory = smoke.parent().expect("smoke script must have a parent");
    run(Command::new("python").arg("-B").arg("-c").arg(script).arg(
        tool_directory
            .to_str()
            .expect("tool directory must be UTF-8"),
    ))
}

fn run_package(repository: &Path, headless: &Path, desktop: &Path, output: &Path) -> Output {
    run(Command::new("python")
        .arg("-B")
        .arg(package_script())
        .args([
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
            "--output",
            output.to_str().expect("fixture path must be UTF-8"),
        ]))
}

fn run_transport(repository: &Path, archive: &Path, output: &Path) -> Output {
    run(Command::new("python")
        .arg("-B")
        .arg(package_script())
        .args([
            "transport",
            "--repository-root",
            repository.to_str().expect("fixture path must be UTF-8"),
            "--archive",
            archive.to_str().expect("fixture path must be UTF-8"),
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

fn create_repository_fixture(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
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
    fs::write(&headless, b"headless binary").expect("headless fixture must be writable");
    fs::write(&desktop, b"desktop binary").expect("desktop fixture must be writable");

    (repository, headless, desktop)
}

#[test]
fn package_creates_a_fixed_checkout_free_payload() {
    let temporary = TemporaryDirectory::new("valid-package");
    let (repository, headless, desktop) = create_repository_fixture(temporary.path());
    let archive = temporary.path().join("candidate.zip");

    let packaged = run_package(&repository, &headless, &desktop, &archive);

    assert!(
        packaged.status.success(),
        "package creation failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );
    assert!(archive.is_file(), "package creation must write one archive");

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

    let transport = temporary.path().join("candidate-transport");
    let transported = run_transport(&repository, &archive, &transport);
    assert!(
        transported.status.success(),
        "candidate transport creation failed: {}",
        String::from_utf8_lossy(&transported.stderr)
    );
    assert!(
        transport
            .join("verification/reference-game/tools/smoke_artifact.py")
            .is_file()
    );
    assert!(
        transport
            .join("verification/reference-game/packaging/package-layout-v1.json")
            .is_file()
    );
    assert!(!transport.join("bundle-manifest.json").exists());
    assert!(!transport.join("candidate/receipt.json").exists());
    let transported_archive = transport.join("candidate/candidate.zip");
    assert!(transported_archive.is_file());
    let transported_smoke = transport.join("verification/reference-game/tools/smoke_artifact.py");
    let transport_verify = run(Command::new("python")
        .arg("-B")
        .arg(transported_smoke)
        .args([
            "verify",
            "--archive",
            transported_archive
                .to_str()
                .expect("transport archive path must be UTF-8"),
            "--expected-platform",
            PLATFORM,
            "--expected-source-revision",
            SOURCE_REVISION,
        ]));
    assert!(
        transport_verify.status.success(),
        "checkout-free transport verification failed: {}",
        String::from_utf8_lossy(&transport_verify.stderr)
    );
}

#[test]
fn transported_archive_identity_is_checked_by_the_archive_itself() {
    let temporary = TemporaryDirectory::new("transported-archive-identity");
    let (repository, headless, desktop) = create_repository_fixture(temporary.path());
    let archive = temporary.path().join("candidate.zip");
    let packaged = run_package(&repository, &headless, &desktop, &archive);
    assert!(
        packaged.status.success(),
        "valid package creation failed: {}",
        String::from_utf8_lossy(&packaged.stderr)
    );

    let wrong_revision = run_smoke(&[
        Path::new("verify"),
        Path::new("--archive"),
        &archive,
        Path::new("--expected-platform"),
        Path::new(PLATFORM),
        Path::new("--expected-source-revision"),
        Path::new("fedcba9876543210fedcba9876543210fedcba98"),
    ]);
    assert!(!wrong_revision.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_revision.stderr).contains("revision does not match"),
        "unexpected archive identity rejection: {}",
        String::from_utf8_lossy(&wrong_revision.stderr)
    );
}

#[test]
fn package_rejects_output_inside_the_source_checkout() {
    let temporary = TemporaryDirectory::new("output-inside-source");
    let (repository, headless, desktop) = create_repository_fixture(temporary.path());
    let archive = repository.join("candidate.zip");

    let packaged = run_package(&repository, &headless, &desktop, &archive);

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
    let (repository, _headless, desktop) = create_repository_fixture(temporary.path());
    let headless_directory = temporary.path().join("not-a-binary");
    fs::create_dir(&headless_directory).expect("fixture directory must be creatable");
    let archive = temporary.path().join("candidate.zip");

    let packaged = run_package(&repository, &headless_directory, &desktop, &archive);

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
    let (repository, headless, desktop) = create_repository_fixture(temporary.path());
    let valid_archive = temporary.path().join("valid.zip");
    let packaged = run_package(&repository, &headless, &desktop, &valid_archive);
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
    assert isolated_cwd == isolated_root / "cwd"
    assert "CARGO_HOME" not in isolated_environment
    assert ".cargo" not in isolated_environment["PATH"].lower()
    assert smoke.parse_environment(["nara_wgpu_force_fallback=1"]) == {
        "NARA_WGPU_FORCE_FALLBACK": "1"
    }
    for protected in ["home=outside", "Temp=outside", "cargo_home=outside"]:
        try:
            smoke.parse_environment([protected])
        except smoke.ArtifactError as error:
            assert "override is invalid" in str(error), str(error)
        else:
            raise AssertionError(f"protected environment alias was accepted: {protected}")
    try:
        smoke.parse_environment(["NARA_TEST=one", "nara_test=two"])
    except smoke.ArtifactError as error:
        assert "override is duplicated" in str(error), str(error)
    else:
        raise AssertionError("case-insensitive duplicate environment keys were accepted")

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
    let result = run_smoke_inline(script);

    assert!(
        result.status.success(),
        "bounded smoke runner contract failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn smoke_runs_use_unique_roots_with_state_outside_the_extracted_package() {
    let script = r#"
from pathlib import Path
import sys
import tempfile

sys.path.insert(0, sys.argv[1])
import smoke_artifact as smoke

with tempfile.TemporaryDirectory(prefix="nara-smoke-root-policy-") as temporary:
    work_parent = Path(temporary).resolve()
    roots = [smoke.create_smoke_root(work_parent) for _ in range(2)]
    assert roots[0] != roots[1]
    for root in roots:
        assert root.parent == work_parent
        assert root.name.startswith(".nara-smoke-")
        environment, cwd = smoke.smoke_environment(root, {})
        package_root = root / "consumer" / "nara-reference-game"
        state_paths = {
            Path(environment["HOME"]),
            Path(environment["TEMP"]),
            Path(environment["TMP"]),
            Path(environment["TMPDIR"]),
            cwd,
        }
        assert state_paths == {root / "home", root / "tmp", root / "cwd"}
        assert all(path.is_dir() for path in state_paths)
        assert all(not smoke.package.path_is_within(path, package_root) for path in state_paths)
"#;
    let result = run_smoke_inline(script);

    assert!(
        result.status.success(),
        "unique smoke-root contract failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn smoke_runner_uses_the_formal_desktop_and_retires_unique_work_roots() {
    let script = r#"
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace

sys.path.insert(0, sys.argv[1])
import smoke_artifact as smoke

validated = SimpleNamespace(
    archive_sha256="0" * 64,
    archive_size_bytes=1,
    layout=SimpleNamespace(package_root="nara-reference-game"),
    manifest={
        "package": {
            "platform": "windows-x86_64",
            "version": "0.1.0",
        },
        "source_revision": "0" * 40,
        "limits": {
            "file_count": 3,
            "expanded_bytes": 3,
        },
        "layout": {
            "headless": "bin/headless.exe",
            "desktop": "bin/desktop.exe",
        }
    },
)
smoke.extract_archive = lambda *args, **kwargs: validated
commands = []

def run_fixture(command, cwd, environment, subject):
    command = tuple(str(part) for part in command)
    commands.append((command, subject))
    if subject == "headless candidate":
        return json.dumps({"schema": smoke.HEADLESS_SUMMARY_SCHEMA}).encode("ascii")
    if subject == "desktop candidate":
        assert Path(command[-2]).name == "desktop.exe", command
        assert command[-1] == "--candidate-smoke", command
        raise smoke.ArtifactError("damaged formal desktop")
    raise AssertionError((command, subject))

smoke.run_bounded = run_fixture
with tempfile.TemporaryDirectory(prefix="nara-formal-desktop-policy-") as temporary:
    work_parent = Path(temporary)
    try:
        smoke.smoke_candidate(Path("unused.zip"), work_parent, "[]", [])
    except smoke.ArtifactError as error:
        assert str(error) == "damaged formal desktop", str(error)
    else:
        raise AssertionError("a working legacy probe masked the damaged formal desktop")
    assert not any(work_parent.iterdir()), "failed smoke must retire its unique work root"

    def successful_run_fixture(command, cwd, environment, subject):
        if subject == "headless candidate":
            return json.dumps({"schema": smoke.HEADLESS_SUMMARY_SCHEMA}).encode("ascii")
        if subject == "desktop candidate":
            return smoke.DESKTOP_SMOKE_SUCCESS
        raise AssertionError((command, subject))

    smoke.run_bounded = successful_run_fixture
    result = smoke.smoke_candidate(Path("unused.zip"), work_parent, "[]", [])
    assert result["desktop"] == "candidate-smoke-completed", result
    assert not any(work_parent.iterdir()), "successful smoke must retire its unique work root"

    remove_temporary = smoke.safe_remove_temporary
    smoke.safe_remove_temporary = lambda *args, **kwargs: False
    try:
        smoke.smoke_candidate(Path("unused.zip"), work_parent, "[]", [])
    except smoke.ArtifactError as error:
        assert str(error) == "smoke work root could not be retired", str(error)
    else:
        raise AssertionError("cleanup failure must reject candidate smoke")
    finally:
        smoke.safe_remove_temporary = remove_temporary
        abandoned_roots = list(work_parent.iterdir())
        assert len(abandoned_roots) == 1, abandoned_roots
        assert remove_temporary(
            abandoned_roots[0], work_parent, smoke.SMOKE_TEMPORARY_PREFIX
        )

assert any(subject == "desktop candidate" for _, subject in commands)
assert all(
    not any("desktop-render-probe" in part for part in command)
    for command, _ in commands
)
"#;
    let result = run_smoke_inline(script);

    assert!(
        result.status.success(),
        "formal desktop smoke contract failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
