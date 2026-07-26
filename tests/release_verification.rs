use serde_json::Value;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
            "nara-release-verification-{label}-{}-{timestamp}-{sequence}",
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

fn verifier() -> PathBuf {
    repository_root().join("reference-game/tools/verify_release.py")
}

fn approval_fixture() -> PathBuf {
    repository_root().join("tests/fixtures/release/approval.json")
}

fn trusted_fixture() -> PathBuf {
    repository_root().join("tests/fixtures/release/trusted-input.json")
}

fn approval_schema() -> PathBuf {
    repository_root()
        .join("docs/benchmarks/data/approvals/v1/reference-game-pre-release.schema.json")
}

fn manifest_schema() -> PathBuf {
    repository_root().join("docs/benchmarks/data/approvals/v1/publication-manifest.schema.json")
}

fn trusted_schema() -> PathBuf {
    repository_root().join("docs/benchmarks/data/approvals/v1/release-trusted-input.schema.json")
}

fn run(arguments: &[&str]) -> Output {
    Command::new("python")
        .arg("-B")
        .arg(verifier())
        .args(arguments)
        .output()
        .expect("release verifier process must start")
}

fn build_manifest(approval: &Path, trusted: &Path, output: &Path) -> Output {
    run(&[
        "build-manifest",
        "--approval",
        approval.to_str().expect("approval path must be UTF-8"),
        "--trusted-input",
        trusted.to_str().expect("trusted input path must be UTF-8"),
        "--output",
        output.to_str().expect("manifest path must be UTF-8"),
    ])
}

fn copy_fixture(root: &Path, name: &str, source: &Path) -> PathBuf {
    let destination = root.join(name);
    fs::copy(source, &destination).expect("fixture must be copied");
    destination
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("JSON output must be readable"))
        .expect("JSON output must be valid")
}

fn replace_once(path: &Path, from: &str, to: &str) {
    let source = fs::read_to_string(path).expect("fixture must be readable");
    assert!(source.contains(from), "fixture mutation source must exist");
    fs::write(path, source.replacen(from, to, 1)).expect("fixture mutation must be writable");
}

#[test]
fn release_policy_schema_is_checked_before_manifest_build() {
    let result = run(&[
        "verify-policy",
        "--approval-schema",
        approval_schema()
            .to_str()
            .expect("approval schema path must be UTF-8"),
        "--manifest-schema",
        manifest_schema()
            .to_str()
            .expect("manifest schema path must be UTF-8"),
        "--trusted-input-schema",
        trusted_schema()
            .to_str()
            .expect("trusted-input schema path must be UTF-8"),
    ]);
    assert!(
        result.status.success(),
        "release policy check failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output: Value = serde_json::from_slice(&result.stdout).expect("policy result must be JSON");
    assert_eq!(output["status"], "policy_valid");
}

#[test]
fn valid_approval_and_trusted_input_produce_bounded_publication_manifest() {
    let temporary = TemporaryDirectory::new("valid");
    let output = temporary.path().join("publication-manifest.json");
    let approval = read_json(&approval_fixture());
    assert!(
        approval["publisher"].get("source_revision").is_none(),
        "an ancestor approval cannot predict its future publisher revision"
    );
    let result = build_manifest(&approval_fixture(), &trusted_fixture(), &output);
    assert!(
        result.status.success(),
        "manifest construction failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let manifest = read_json(&output);
    assert_eq!(
        manifest["schema"],
        "nara.reference-game.publication-manifest-v1"
    );
    assert_eq!(manifest["format_version"], 1);
    assert_eq!(manifest["tag"]["name"], "v0.1.0-pre.1");
    assert_eq!(
        manifest["candidates"]
            .as_array()
            .expect("manifest candidates must be an array")
            .len(),
        2
    );
    assert_eq!(manifest["release"]["draft"], true);
    assert_eq!(manifest["release"]["prerelease"], true);
    assert_eq!(
        manifest["publisher"]["source_revision"],
        "abababababababababababababababababababab"
    );
    assert!(
        manifest["checksum_file"]["content"]
            .as_str()
            .is_some_and(|content| content.contains("nara-reference-game-linux-x86_64.zip"))
    );
    assert!(fs::metadata(&output).expect("manifest must exist").len() <= 65_536);
}

#[test]
fn release_verifier_rejects_non_publish_and_untrusted_identity_changes() {
    let temporary = TemporaryDirectory::new("rejections");
    let approval = copy_fixture(temporary.path(), "approval.json", &approval_fixture());
    let trusted = copy_fixture(temporary.path(), "trusted.json", &trusted_fixture());
    let output = temporary.path().join("manifest.json");

    replace_once(
        &approval,
        "\"decision\": \"Publish\"",
        "\"decision\": \"Redirect\"",
    );
    let result = build_manifest(&approval, &trusted, &output);
    assert!(!result.status.success());

    fs::copy(approval_fixture(), &approval).expect("approval fixture must reset");
    replace_once(
        &trusted,
        "\"target_sha\": \"1111111111111111111111111111111111111111\"",
        "\"target_sha\": \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"",
    );
    let result = build_manifest(&approval, &trusted, &output);
    assert!(!result.status.success());
}

#[test]
fn release_verifier_rejects_publisher_definition_substitution() {
    let temporary = TemporaryDirectory::new("publisher-binding");
    let approval = copy_fixture(temporary.path(), "approval.json", &approval_fixture());
    let trusted = copy_fixture(temporary.path(), "trusted.json", &trusted_fixture());
    let output = temporary.path().join("manifest.json");

    replace_once(
        &trusted,
        "\"definition_sha256\": \"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\"",
        "\"definition_sha256\": \"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\"",
    );
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success(),
        "a caller-controlled publisher definition digest must not be accepted"
    );
}

#[test]
fn release_verifier_rejects_candidate_digest_expiry_and_manifest_reuse() {
    let temporary = TemporaryDirectory::new("candidate");
    let approval = copy_fixture(temporary.path(), "approval.json", &approval_fixture());
    let trusted = copy_fixture(temporary.path(), "trusted.json", &trusted_fixture());
    let output = temporary.path().join("manifest.json");

    replace_once(
        &trusted,
        "\"sha256:7777777777777777777777777777777777777777777777777777777777777777\"",
        "\"sha256:8888888888888888888888888888888888888888888888888888888888888888\"",
    );
    let result = build_manifest(&approval, &trusted, &output);
    assert!(!result.status.success());

    fs::copy(trusted_fixture(), &trusted).expect("trusted fixture must reset");
    replace_once(
        &trusted,
        "\"now_unix_seconds\": 1785000000",
        "\"now_unix_seconds\": 1790000000",
    );
    let result = build_manifest(&approval, &trusted, &output);
    assert!(!result.status.success());

    fs::copy(trusted_fixture(), &trusted).expect("trusted fixture must reset");
    let first = build_manifest(&approval, &trusted, &output);
    assert!(
        first.status.success(),
        "first manifest construction failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let second = build_manifest(&approval, &trusted, &output);
    assert!(!second.status.success());
}

#[test]
fn release_verifier_rejects_noncanonical_unknown_and_unpinned_inputs() {
    let temporary = TemporaryDirectory::new("shape");
    let approval = copy_fixture(temporary.path(), "approval.json", &approval_fixture());
    let trusted = copy_fixture(temporary.path(), "trusted.json", &trusted_fixture());
    let output = temporary.path().join("manifest.json");

    let compact: Value =
        serde_json::from_slice(&fs::read(&approval).expect("approval must be readable"))
            .expect("approval must decode");
    fs::write(
        &approval,
        serde_json::to_vec(&compact).expect("compact JSON must encode"),
    )
    .expect("compact approval must be writable");
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success()
    );

    fs::copy(approval_fixture(), &approval).expect("approval fixture must reset");
    replace_once(
        &approval,
        "  \"format_version\": 1,",
        "  \"format_version\": 1,\n  \"unexpected\": true,",
    );
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success()
    );

    fs::copy(approval_fixture(), &approval).expect("approval fixture must reset");
    replace_once(
        &trusted,
        "\"sha256\": \"b7c80f81e780841d312adfd32611b8ef35ccbb63c60bcd0723c96f71b672ab54\"",
        "\"sha256\": \"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\"",
    );
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success()
    );
}

#[test]
fn release_verifier_rejects_unprotected_tag_and_mutable_release_repository() {
    let temporary = TemporaryDirectory::new("immutability");
    let approval = copy_fixture(temporary.path(), "approval.json", &approval_fixture());
    let trusted = copy_fixture(temporary.path(), "trusted.json", &trusted_fixture());
    let output = temporary.path().join("manifest.json");

    replace_once(&trusted, "\"protected\": true", "\"protected\": false");
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success()
    );

    fs::copy(trusted_fixture(), &trusted).expect("trusted fixture must reset");
    replace_once(
        &trusted,
        "\"immutable_releases_enabled\": true",
        "\"immutable_releases_enabled\": false",
    );
    assert!(
        !build_manifest(&approval, &trusted, &output)
            .status
            .success()
    );
}
