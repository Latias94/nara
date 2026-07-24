use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PYTHON_INVARIANT_TEST: &str = r#"
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import time

tools = Path(sys.argv[1]) / "reference-game" / "tools"
sys.path.insert(0, str(tools))

import candidate_measurement as measurement
import smoke_artifact


def run_python(source, marker_event, timeout_seconds=5.0):
    return measurement.observe_process(
        [sys.executable, "-B", "-c", source],
        Path.cwd(),
        dict(os.environ),
        marker_event,
        timeout_seconds=timeout_seconds,
    )


headless_event = "headless_first_authoritative_tick"
headless_marker = measurement.startup_marker_line(headless_event)
summary = (
    json.dumps(
        {
            "schema": smoke_artifact.HEADLESS_SUMMARY_SCHEMA,
            "outcome": "completed",
            "tick": 12,
            "score": 300,
            "player_hit_points": 4,
            "enemies_remaining": 0,
            "projectiles_remaining": 1,
        },
        separators=(",", ":"),
    ).encode("ascii")
    + b"\n"
)
valid = run_python(
    "import sys;sys.stdout.buffer.write("
    + repr(headless_marker + summary)
    + ");sys.stdout.buffer.flush()",
    headless_event,
)
valid = measurement.with_output_validation(
    valid,
    headless_event,
    measurement.validate_headless_stdout,
)
assert valid.succeeded
assert valid.marker_count == 1
assert valid.marker_duration_ns is not None
assert valid.marker_duration_ns >= 0
assert valid.stderr == b""

duplicate = run_python(
    "import sys;sys.stdout.buffer.write("
    + repr(headless_marker + headless_marker + summary)
    + ");sys.stdout.buffer.flush()",
    headless_event,
)
assert duplicate.failure == "startup_marker_duplicate"
assert duplicate.marker_count == 2

overflow = run_python(
    "import sys;sys.stdout.buffer.write(b'x' * "
    + str(measurement.MAX_COMBINED_OUTPUT_BYTES + 1)
    + ");sys.stdout.buffer.flush()",
    headless_event,
)
assert overflow.failure == "process_output_limit"
assert len(overflow.stdout) + len(overflow.stderr) <= measurement.MAX_COMBINED_OUTPUT_BYTES

timeout_started = time.monotonic()
timed_out = run_python(
    "import time;time.sleep(30)",
    headless_event,
    timeout_seconds=0.05,
)
assert timed_out.failure == "process_timeout"
assert time.monotonic() - timeout_started < 5.0

layout = smoke_artifact.package.load_layout()
validated = smoke_artifact.ValidatedArchive(
    archive_sha256="a" * 64,
    archive_size_bytes=1234,
    layout=layout,
    manifest={
        "package": {
            "name": "nara-reference-game",
            "version": "0.1.0",
            "platform": "windows-x86_64",
        },
        "source_revision": "0" * 40,
        "limits": {
            "file_count": 4,
            "expanded_bytes": 5678,
        },
    },
    descriptors={},
)
spec = measurement.CandidateProcessSpec(
    collector_id="test_component_v1",
    layout_key="headless",
    entry_point="bin/headless",
    marker_event=headless_event,
    component_boundary_id="headless_marker_received_v1",
    command_prefix=(),
    command_arguments=(),
    validate_stdout=measurement.validate_headless_stdout,
)
canary = b"NARA_RESTRICTED_RAW_CANARY_4f850afe"
successful_raw = measurement.ProcessObservation(
    started_at_unix_ns=1,
    completed_at_unix_ns=2,
    exit_code=0,
    failure=None,
    marker_count=1,
    marker_duration_ns=17,
    stdout=canary,
    stderr=b"",
)
prepared = measurement.PreparedCandidate(
    validated=validated,
    package_root=Path("unused"),
    cwd=Path("unused"),
    environment={},
)
payload = measurement.raw_observation_payload(
    prepared,
    spec,
    7,
    successful_raw,
    successful_raw,
)
encoded = json.dumps(payload, sort_keys=True).encode("utf-8")
assert canary not in encoded
assert payload["decision"] == "not_evaluated"
assert payload["status"] == "observed"
assert payload["measurement"]["sample_index"] == 7
assert payload["measurement"]["value_ns"] == 17

with tempfile.TemporaryDirectory(prefix="nara-candidate-measurement-test-") as root_value:
    root = Path(root_value)
    work_root = root / "work"
    work_root.mkdir()
    output = measurement.validate_output_destination(root / "published", work_root)
    measurement.publish_raw_observation(
        output,
        payload,
        successful_raw,
        successful_raw,
    )
    observation_path = output / measurement.RAW_OBSERVATION_FILENAME
    observation_bytes = observation_path.read_bytes()
    assert canary not in observation_bytes
    published = json.loads(observation_bytes)
    for phase in ("warmup", "sample"):
        for stream in ("stdout", "stderr"):
            descriptor = published[phase][stream]
            raw = (output / descriptor["path"]).read_bytes()
            assert descriptor["bytes"] == len(raw)
            assert descriptor["sha256"] == hashlib.sha256(raw).hexdigest()
    assert (output / published["sample"]["stdout"]["path"]).read_bytes() == canary
    try:
        measurement.validate_output_destination(output, work_root)
    except measurement.CandidateMeasurementError:
        pass
    else:
        raise AssertionError("an existing observation destination must be rejected")

try:
    measurement.validate_sample_index(0)
except measurement.CandidateMeasurementError:
    pass
else:
    raise AssertionError("sample index zero must be rejected")
"#;

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
            "nara-candidate-measurement-{label}-{}-{timestamp}-{sequence}",
            std::process::id()
        ));

        fs::create_dir(&path).expect("test temporary directory must be creatable");
        Self { path }
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let temporary_root = env::temp_dir()
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
            candidate.starts_with(&temporary_root)
                && candidate != temporary_root
                && !candidate.starts_with(&repository),
            "test cleanup must remain inside its own system temporary directory: {candidate:?}"
        );
        fs::remove_dir_all(candidate).expect("test temporary directory must be removable");
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .expect("test command must start successfully")
}

#[test]
fn candidate_process_observations_are_bounded_and_raw() {
    let temporary = TemporaryDirectory::new("process");
    let result = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(PYTHON_INVARIANT_TEST)
        .arg(repository_root())
        .current_dir(&temporary.path));

    assert!(
        result.status.success(),
        "candidate measurement invariant test failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn candidate_collectors_do_not_claim_evidence_or_execute_through_a_shell() {
    let tools = repository_root().join("reference-game/tools");
    let common = fs::read_to_string(tools.join("candidate_measurement.py"))
        .expect("candidate measurement helper must be readable");
    let headless = fs::read_to_string(tools.join("measure_headless_iteration.py"))
        .expect("headless collector must be readable");
    let desktop = fs::read_to_string(tools.join("measure_desktop_product.py"))
        .expect("desktop collector must be readable");

    for (name, source) in [
        ("common helper", common.as_str()),
        ("headless collector", headless.as_str()),
        ("desktop collector", desktop.as_str()),
    ] {
        assert!(
            !source.contains("shell=True") && !source.contains("shell = True"),
            "{name} must not invoke a command shell"
        );
        assert!(
            !source.contains("eval(") && !source.contains("exec("),
            "{name} must not evaluate transferred code"
        );
    }

    assert!(common.contains("\"decision\": \"not_evaluated\""));
    assert!(common.contains("\"credential_free_untrusted\""));
    assert!(common.contains("MAX_COMBINED_OUTPUT_BYTES"));
    assert!(!common.contains("import statistics"));
    assert!(!common.contains("import numpy"));
    assert!(!common.contains("def percentile"));
    assert!(headless.contains("layout_key=\"headless\""));
    assert!(headless.contains("headless_first_authoritative_tick"));
    assert!(desktop.contains("layout_key=\"desktop_probe\""));
    assert!(desktop.contains("desktop_first_playable_present"));
    assert!(!desktop.contains("layout_key=\"desktop\""));
}
