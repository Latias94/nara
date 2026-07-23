#[path = "support/first_playable_evidence.rs"]
mod evidence;

use evidence::{
    EnvelopeLimits, EvidenceEnvelope, EvidenceError, calibration_expected_identity,
    canonical_json_bytes, decode_evidence, expected_transfer, load_protocol_fixture,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const NORMALIZED_SCHEMA: &str = "nara.reference-game.normalized-evidence-v1";
const NORMALIZER_ID: &str = "nara_reference_game_ingest_v1";
const OUTER_VALIDATION_SCOPE: &str = "outer_transfer_and_structure_v1";
const CANARY: &str = "nara_evidence_canary_7f2b";

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
            "nara-evidence-ingest-policy-{label}-{}-{timestamp}-{sequence}",
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

fn ingest_script() -> PathBuf {
    repository_root().join("reference-game/tools/ingest_evidence.py")
}

fn fixture_script() -> PathBuf {
    repository_root().join("tests/support/evidence_ingest_fixture.py")
}

fn schema_path() -> PathBuf {
    repository_root().join("docs/benchmarks/data/envelope/v1/normalized-evidence.schema.json")
}

fn calibration_envelope() -> PathBuf {
    repository_root().join("docs/benchmarks/data/envelope/v1/calibration-review.json")
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .expect("test command must start successfully")
}

fn prepare_fixture(root: &Path) -> (PathBuf, PathBuf) {
    let input = root.join("input");
    fs::create_dir(&input).expect("fixture input directory must be creatable");
    let envelope = input.join("envelope.json");
    let expected = input.join("expected.json");
    fs::copy(calibration_envelope(), &envelope).expect("calibration envelope must copy");
    let result = run(Command::new("python")
        .arg("-B")
        .arg(fixture_script())
        .args([
            "prepare",
            envelope.to_str().expect("fixture path must be UTF-8"),
            expected.to_str().expect("fixture path must be UTF-8"),
            schema_path().to_str().expect("schema path must be UTF-8"),
        ]));
    assert!(
        result.status.success(),
        "fixture preparation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    (envelope, expected)
}

fn normalize(envelope: &Path, expected: &Path, output: &Path, extra: &[&str]) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(ingest_script()).args([
        "normalize",
        "--envelope",
        envelope.to_str().expect("fixture path must be UTF-8"),
        "--expected",
        expected.to_str().expect("fixture path must be UTF-8"),
        "--schema",
        schema_path().to_str().expect("schema path must be UTF-8"),
        "--output",
        output.to_str().expect("fixture path must be UTF-8"),
    ]);
    command.args(extra);
    run(&mut command)
}

fn mutate(mode: &str, envelope: &Path, expected: &Path) {
    let result = run(Command::new("python")
        .arg("-B")
        .arg(fixture_script())
        .args([
            "mutate",
            mode,
            envelope.to_str().expect("fixture path must be UTF-8"),
            expected.to_str().expect("fixture path must be UTF-8"),
            schema_path().to_str().expect("schema path must be UTF-8"),
            CANARY,
        ]));
    assert!(
        result.status.success(),
        "fixture mutation failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("output must be readable"))
        .expect("output must be valid JSON")
}

fn validate_normalized_evidence_semantics(
    normalized: &Value,
    raw_envelope: &[u8],
) -> Result<(), EvidenceError> {
    let protocol = load_protocol_fixture().map_err(|_| EvidenceError::Decode)?;
    let envelope = serde_json::from_value::<EvidenceEnvelope>(normalized["evidence"].clone())
        .map_err(|_| EvidenceError::Decode)?;
    if canonical_json_bytes(&envelope) != raw_envelope {
        return Err(EvidenceError::NonCanonical);
    }
    let expected_transfer = expected_transfer(evidence::EVIDENCE_TRANSFER_PATH, raw_envelope);
    let expected_identity = calibration_expected_identity(&protocol);

    decode_evidence(
        raw_envelope,
        EnvelopeLimits::from(&protocol.evidence),
        &expected_transfer,
        &expected_identity,
        &protocol,
    )
    .map(|_| ())
}

#[test]
fn committed_normalized_outer_schema_is_closed_and_versioned() {
    let schema = read_json(&schema_path());
    assert_eq!(
        schema["$id"],
        Value::String(NORMALIZED_SCHEMA.to_owned()),
        "the checked-in schema must name the one normalized-evidence contract"
    );
    assert_eq!(schema["type"], Value::String("object".to_owned()));
    assert_eq!(schema["additionalProperties"], Value::Bool(false));
    assert!(
        schema["required"]
            .as_array()
            .is_some_and(|fields| fields.iter().any(|field| field == "evidence"))
    );
}

#[test]
fn valid_envelope_normalizes_without_candidate_execution_or_repository_mutation() {
    let temporary = TemporaryDirectory::new("valid");
    let (envelope, expected) = prepare_fixture(temporary.path());
    let output_root = temporary.path().join("output");
    fs::create_dir(&output_root).expect("fixture output directory must be creatable");
    let output = output_root.join("normalized.json");

    let result = normalize(&envelope, &expected, &output, &[]);
    assert!(
        result.status.success(),
        "normalization failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        output.is_file(),
        "normalization must publish exactly one output file"
    );
    let normalized = read_json(&output);
    assert_eq!(normalized["schema"], NORMALIZED_SCHEMA);
    assert_eq!(normalized["format_version"], 1);
    assert_eq!(normalized["normalizer"]["id"], NORMALIZER_ID);
    assert_eq!(
        normalized["normalizer"]["validation_scope"], OUTER_VALIDATION_SCOPE,
        "the artifact must state that semantic protocol verification remains a separate trusted gate"
    );
    assert_eq!(
        normalized["evidence"]["identity"],
        read_json(&envelope)["identity"],
        "the normalizer may not rewrite collector identity"
    );
    assert_eq!(
        normalized["input"]["candidate"]["receipt"]["platform"], "windows-x86_64",
        "candidate provenance must retain the U10 package receipt shape"
    );
    assert_eq!(
        normalized["input"]["candidate"]["receipt"]["source_revision"],
        normalized["evidence"]["identity"]["source_revision"],
        "the candidate receipt and evidence identity must bind one reviewed source revision"
    );
}

#[test]
fn normalized_evidence_passes_the_existing_u22_semantic_oracle() {
    let temporary = TemporaryDirectory::new("semantic-valid");
    let (envelope, expected) = prepare_fixture(temporary.path());
    let output_root = temporary.path().join("output");
    fs::create_dir(&output_root).expect("fixture output directory must be creatable");
    let output = output_root.join("normalized.json");

    let result = normalize(&envelope, &expected, &output, &[]);
    assert!(
        result.status.success(),
        "normalization failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let raw_envelope = fs::read(&envelope).expect("fixture envelope must be readable");
    validate_normalized_evidence_semantics(&read_json(&output), &raw_envelope)
        .expect("outer-normalized evidence must still satisfy the complete U22 semantic oracle");
}

#[test]
fn outer_normalization_cannot_promote_a_bad_payload_digest_to_approval() {
    let temporary = TemporaryDirectory::new("payload-digest");
    let (envelope, expected) = prepare_fixture(temporary.path());
    mutate("payload-digest-mismatch", &envelope, &expected);
    let output_root = temporary.path().join("output");
    fs::create_dir(&output_root).expect("fixture output directory must be creatable");
    let output = output_root.join("normalized.json");

    let result = normalize(&envelope, &expected, &output, &[]);
    assert!(
        result.status.success(),
        "outer normalization should accept a shape-valid envelope whose raw transfer was independently rebound"
    );
    let normalized = read_json(&output);
    assert_eq!(
        normalized["normalizer"]["validation_scope"], OUTER_VALIDATION_SCOPE,
        "the artifact must not claim that outer normalization completed semantic validation"
    );
    assert_eq!(
        validate_normalized_evidence_semantics(
            &normalized,
            &fs::read(&envelope).expect("fixture envelope must be readable"),
        ),
        Err(EvidenceError::PayloadDigest),
        "a payload-digest mismatch may not become approval evidence after outer normalization"
    );
}

#[test]
fn tampered_or_unsafe_envelopes_reject_without_publication_or_canary_echo() {
    for mode in [
        "tamper-after-expectation",
        "unsafe-identifier",
        "unknown-field",
        "candidate-source-mismatch",
        "expected-environment-drift",
    ] {
        let temporary = TemporaryDirectory::new(mode);
        let (envelope, expected) = prepare_fixture(temporary.path());
        mutate(mode, &envelope, &expected);
        let output_root = temporary.path().join("output");
        fs::create_dir(&output_root).expect("fixture output directory must be creatable");
        let output = output_root.join("normalized.json");

        let result = normalize(&envelope, &expected, &output, &[]);
        assert!(
            !result.status.success(),
            "{mode} must reject instead of normalizing untrusted evidence"
        );
        assert!(
            !output.exists(),
            "{mode} must not leave a partially published normalized record"
        );
        assert!(
            !String::from_utf8_lossy(&result.stderr).contains(CANARY),
            "{mode} diagnostics must not echo untrusted sensitive input"
        );
    }
}

#[test]
fn normalizer_refuses_existing_or_source_adjacent_outputs_and_encoded_budget_excess() {
    let temporary = TemporaryDirectory::new("destinations");
    let (envelope, expected) = prepare_fixture(temporary.path());
    let existing = temporary.path().join("existing.json");
    fs::write(&existing, b"prior bytes").expect("existing output must be writable");

    let existing_result = normalize(&envelope, &expected, &existing, &[]);
    assert!(
        !existing_result.status.success(),
        "normalizer must never overwrite an existing output"
    );
    assert_eq!(
        fs::read(&existing).expect("existing output must remain readable"),
        b"prior bytes"
    );

    let adjacent = envelope
        .parent()
        .expect("fixture envelope must have a parent")
        .join("nested")
        .join("normalized.json");
    let adjacent_result = normalize(&envelope, &expected, &adjacent, &[]);
    assert!(
        !adjacent_result.status.success(),
        "normalizer output must not enter the untrusted evidence root"
    );

    let bounded_root = temporary.path().join("bounded-output");
    fs::create_dir(&bounded_root).expect("bounded output directory must be creatable");
    let bounded = bounded_root.join("normalized.json");
    let budget_result = normalize(
        &envelope,
        &expected,
        &bounded,
        &["--max-envelope-bytes", "1"],
    );
    assert!(
        !budget_result.status.success(),
        "an encoded evidence budget below the actual file size must reject before publication"
    );
    assert!(!bounded.exists());
}

#[test]
fn normalizer_refuses_aliased_input_paths_before_reading_evidence() {
    let temporary = TemporaryDirectory::new("aliased-input");
    let (envelope, expected) = prepare_fixture(temporary.path());
    let input_root = envelope
        .parent()
        .expect("fixture envelope must have a parent");
    let aliased_envelope = input_root.join("..").join("input").join("envelope.json");
    let aliased_expected = input_root.join("..").join("input").join("expected.json");
    let output_root = temporary.path().join("output");
    fs::create_dir(&output_root).expect("fixture output directory must be creatable");
    let output = output_root.join("normalized.json");

    let result = normalize(&aliased_envelope, &aliased_expected, &output, &[]);
    assert!(
        !result.status.success(),
        "an aliased input path must reject before evidence bytes are read"
    );
    assert!(
        !output.exists(),
        "a rejected aliased input must not publish a normalized artifact"
    );
    assert!(envelope.is_file() && expected.is_file());
}

#[cfg(unix)]
#[test]
fn normalizer_refuses_input_reached_through_a_linked_parent_directory() {
    use std::os::unix::fs::symlink;

    let temporary = TemporaryDirectory::new("linked-parent");
    let (envelope, expected) = prepare_fixture(temporary.path());
    let linked_input = temporary.path().join("linked-input");
    symlink(
        envelope
            .parent()
            .expect("fixture envelope must have a parent"),
        &linked_input,
    )
    .expect("test must be able to create an input-directory symlink");
    let output_root = temporary.path().join("output");
    fs::create_dir(&output_root).expect("fixture output directory must be creatable");
    let output = output_root.join("normalized.json");

    let result = normalize(
        &linked_input.join("envelope.json"),
        &linked_input.join("expected.json"),
        &output,
        &[],
    );
    assert!(
        !result.status.success(),
        "a linked parent directory must reject before evidence bytes are read"
    );
    assert!(
        !output.exists(),
        "a rejected linked input must not publish a normalized artifact"
    );
}
