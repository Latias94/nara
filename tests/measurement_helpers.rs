use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const RUN_SCHEMA: &str = "nara.reference-game.first-playable-collection-transport-v1";
const HELPER_RELATIVE_PATH: &str = "reference-game/tools/measure_first_playable.py";
const CATALOG_RELATIVE_PATH: &str =
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json";
const AUTOMATIC_METRICS: &[&str] = &[
    "build.cold_ns",
    "build.incremental_ns",
    "gameplay.headless_wave_success",
    "iteration.body.p50_ns",
    "iteration.body.p95_ns",
    "iteration.data.p50_ns",
    "iteration.data.p95_ns",
    "iteration.structural.p50_ns",
    "iteration.structural.p95_ns",
    "journey.clean_to_headless_wave_ns",
];

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
            "nara-measurement-helper-{label}-{}-{timestamp}-{sequence}",
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
            "test cleanup escaped its system temporary root: {candidate:?}"
        );
        fs::remove_dir_all(candidate).expect("test temporary directory must be removable");
    }
}

#[derive(Clone, Copy)]
enum CollectionMode {
    Success,
    BuildFailure,
    PublicSurfaceTimeout,
    PublicSurfaceOverflow,
    HangingBuild,
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repository_helper() -> PathBuf {
    repository_root().join(HELPER_RELATIVE_PATH)
}

fn subject_helper(subject: &Path) -> PathBuf {
    subject.join(HELPER_RELATIVE_PATH)
}

fn set_diagnostic_log_budget(subject: &Path, byte_budget: usize) {
    let path = subject_helper(subject);
    let source = fs::read_to_string(&path).expect("subject helper must be readable");
    let marker = "MAX_DIAGNOSTIC_LOG_BYTES = 64 * 1024 * 1024";
    assert_eq!(source.matches(marker).count(), 1);
    fs::write(
        path,
        source.replace(marker, &format!("MAX_DIAGNOSTIC_LOG_BYTES = {byte_budget}")),
    )
    .expect("subject helper log budget must be writable");
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .expect("test command must start successfully")
}

fn run_git(directory: &Path, arguments: &[&str]) -> Output {
    let output = run(Command::new("git").current_dir(directory).args(arguments));
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn commit_all(subject: &Path, message: &str) {
    run_git(subject, &["add", "."]);
    run_git(subject, &["commit", "--quiet", "-m", message]);
}

fn write_fixture_catalog(subject: &Path) {
    let source = fs::read(repository_root().join(CATALOG_RELATIVE_PATH))
        .expect("metric catalog must be readable");
    let mut catalog: Value = serde_json::from_slice(&source).expect("metric catalog must be JSON");
    for metric in catalog["metrics"]
        .as_array_mut()
        .expect("metric catalog must contain an array")
    {
        if metric["collector"] == "u14"
            && AUTOMATIC_METRICS.contains(&metric["id"].as_str().unwrap())
        {
            metric["minimum_samples"] = Value::from(1);
        }
    }
    let path = subject.join(CATALOG_RELATIVE_PATH);
    fs::create_dir_all(path.parent().expect("catalog must have a parent"))
        .expect("catalog parent must be creatable");
    let mut bytes = serde_json::to_vec_pretty(&catalog).expect("catalog must encode");
    bytes.push(b'\n');
    fs::write(path, bytes).expect("fixture catalog must be writable");
}

fn create_subject(
    root: &Path,
    label: &str,
    mode: CollectionMode,
    sentinel: Option<&Path>,
) -> PathBuf {
    let subject = root.join(label);
    fs::create_dir(&subject).expect("fixture repository must be creatable");
    run_git(&subject, &["init", "--quiet"]);
    run_git(&subject, &["config", "user.email", "tests@nara.invalid"]);
    run_git(&subject, &["config", "user.name", "Nara Measurement Tests"]);
    run_git(&subject, &["config", "core.autocrlf", "false"]);

    for relative in [
        HELPER_RELATIVE_PATH,
        "reference-game/src/systems.rs",
        "reference-game/scenes/startup.scene.json",
    ] {
        let target = subject.join(relative);
        fs::create_dir_all(target.parent().expect("fixture path must have a parent"))
            .expect("fixture parent must be creatable");
        fs::copy(repository_root().join(relative), target)
            .expect("fixture must copy current reviewed source");
    }
    for relative in [
        "Cargo.toml",
        "reference-game/Cargo.toml",
        "reference-game/Cargo.lock",
        "reference-game/src/bin/headless.rs",
        "reference-game/src/bin/desktop.rs",
        "reference-game/src/bin/desktop_render_probe.rs",
        "reference-game/tests/public_surface.rs",
    ] {
        let target = subject.join(relative);
        fs::create_dir_all(target.parent().expect("fixture path must have a parent"))
            .expect("fixture parent must be creatable");
        fs::write(target, format!("fixture: {relative}\n")).expect("fixture file must be writable");
    }
    write_fixture_catalog(&subject);

    let game = subject.join("reference-game");
    fs::write(game.join("fetch"), "raise SystemExit(0)\n")
        .expect("fake fetch command must be writable");
    let build = match mode {
        CollectionMode::BuildFailure => {
            "print('injected build failure')\nraise SystemExit(7)\n".to_owned()
        }
        CollectionMode::HangingBuild => {
            let sentinel = sentinel.expect("hanging build requires a sentinel");
            let child = format!(
                "from pathlib import Path\nimport time\ntime.sleep(2.0)\nPath({}).write_text('orphan', encoding='utf-8')\n",
                serde_json::to_string(&sentinel.to_string_lossy())
                    .expect("sentinel path must encode")
            );
            format!(
                "import subprocess\nimport sys\nimport time\nsubprocess.Popen([sys.executable, '-c', {}])\ntime.sleep(30)\n",
                serde_json::to_string(&child).expect("child source must encode")
            )
        }
        _ => "raise SystemExit(0)\n".to_owned(),
    };
    fs::write(game.join("build"), build).expect("fake build command must be writable");
    fs::write(
        game.join("run"),
        r#"import json
from pathlib import Path

scene = Path("scenes/startup.scene.json").read_text(encoding="utf-8")
hit_points = 21 if '"value": 21' in scene else 20
print(json.dumps({
    "schema": "nara-reference-game.wave-summary-v1",
    "outcome": "completed",
    "tick": 49,
    "score": 300,
    "player_hit_points": hit_points,
    "enemies_remaining": 0,
    "projectiles_remaining": 4,
}, separators=(",", ":")))
"#,
    )
    .expect("fake run command must be writable");
    let public_surface = match mode {
        CollectionMode::PublicSurfaceTimeout => "import time\ntime.sleep(3)\n".to_owned(),
        CollectionMode::PublicSurfaceOverflow => "print('x' * (1024 * 1024 + 4096))\n".to_owned(),
        _ => "raise SystemExit(0)\n".to_owned(),
    };
    fs::write(game.join("test"), public_surface).expect("fake test command must be writable");
    commit_all(&subject, "measurement fixture");
    subject
}

fn run_helper(helper: &Path, arguments: &[&Path], environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(helper);
    for argument in arguments {
        command.arg(argument);
    }
    for &(key, value) in environment {
        command.env(key, value);
    }
    run(&mut command)
}

fn run_collection(
    subject: &Path,
    output: &Path,
    timeout: &str,
    environment: &[(&str, &Path)],
) -> Output {
    run_helper(
        &subject_helper(subject),
        &[
            Path::new("collect"),
            Path::new("--subject"),
            subject,
            Path::new("--output"),
            output,
            Path::new("--cargo"),
            Path::new("python"),
            Path::new("--command-timeout-seconds"),
            Path::new(timeout),
        ],
        environment,
    )
}

fn run_verify(subject: &Path, output: &Path) -> Output {
    run_helper(
        &subject_helper(subject),
        &[
            Path::new("verify"),
            Path::new("--subject"),
            subject,
            Path::new("--run"),
            output,
        ],
        &[],
    )
}

fn git_status(subject: &Path) -> String {
    String::from_utf8(run_git(subject, &["status", "--porcelain=v1"]).stdout)
        .expect("git status must be UTF-8")
}

fn read_manifest(output: &Path) -> Value {
    serde_json::from_slice(
        &fs::read(output.join("run-manifest.json")).expect("run manifest must exist"),
    )
    .expect("run manifest must be JSON")
}

fn read_records(output: &Path) -> Vec<Value> {
    fs::read_to_string(output.join("raw-samples.jsonl"))
        .expect("raw samples must exist")
        .lines()
        .map(|line| serde_json::from_str(line).expect("raw sample must be JSON"))
        .collect()
}

fn assert_scratch_removed(output: &Path) {
    for name in ["worktree", "target", "cargo-home", "home", "temp"] {
        assert!(
            !output.join(name).exists(),
            "collection scratch {name} must not survive"
        );
    }
}

#[test]
fn collector_must_be_the_committed_helper_inside_the_subject() {
    let temporary = TemporaryDirectory::new("collector-binding");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    let output = temporary.path().join("outside-helper-output");

    let result = run_helper(
        &repository_helper(),
        &[
            Path::new("collect"),
            Path::new("--subject"),
            &subject,
            Path::new("--output"),
            &output,
            Path::new("--cargo"),
            Path::new("python"),
        ],
        &[],
    );

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("executing helper must come from the measurement subject")
    );
    assert!(!output.exists());

    fs::OpenOptions::new()
        .append(true)
        .open(subject_helper(&subject))
        .expect("subject helper must be writable")
        .write_all(b"\n# locally modified helper\n")
        .expect("subject helper mutation must succeed");
    run_git(
        &subject,
        &["update-index", "--assume-unchanged", HELPER_RELATIVE_PATH],
    );
    assert_eq!(git_status(&subject), "");
    let modified_output = temporary.path().join("modified-helper-output");
    let modified = run_helper(
        &subject_helper(&subject),
        &[
            Path::new("collect"),
            Path::new("--subject"),
            &subject,
            Path::new("--output"),
            &modified_output,
        ],
        &[],
    );
    assert!(!modified.status.success());
    assert!(
        String::from_utf8_lossy(&modified.stderr)
            .contains("executing helper bytes must match the subject HEAD blob")
    );
    assert!(!modified_output.exists());
}

#[test]
fn collection_is_isolated_and_transport_verification_ignores_diagnostic_logs() {
    let temporary = TemporaryDirectory::new("collect-success");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    let revision = String::from_utf8(run_git(&subject, &["rev-parse", "HEAD"]).stdout)
        .expect("revision must be UTF-8");
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output, "10", &[]);

    assert!(
        result.status.success(),
        "collection failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert_eq!(git_status(&subject), "");
    assert_eq!(
        String::from_utf8(run_git(&subject, &["rev-parse", "HEAD"]).stdout)
            .expect("revision must be UTF-8"),
        revision
    );
    let worktrees =
        String::from_utf8(run_git(&subject, &["worktree", "list", "--porcelain"]).stdout)
            .expect("worktree list must be UTF-8");
    assert_eq!(worktrees.matches("worktree ").count(), 1);

    let manifest = read_manifest(&output);
    assert_eq!(manifest["schema"], RUN_SCHEMA);
    assert_eq!(manifest["status"], "collected");
    assert_eq!(manifest["source_revision"], revision.trim());
    assert!(manifest.get("decision").is_none());
    assert!(manifest.get("not_collected").is_none());
    assert_eq!(manifest["diagnostic_logs"]["canonical"], false);

    let records = read_records(&output);
    assert_eq!(records.len(), AUTOMATIC_METRICS.len());
    assert_eq!(
        manifest["raw_samples"]["count"].as_u64().unwrap() as usize,
        records.len()
    );
    for metric in AUTOMATIC_METRICS {
        let record = records
            .iter()
            .find(|record| record["metric_id"] == *metric)
            .unwrap_or_else(|| panic!("missing metric {metric}"));
        assert!(record["sample_value"].is_number());
        assert_eq!(record["command"]["exit_code"], 0);
        assert_eq!(record["command"]["timed_out"], false);
        assert_eq!(record["command"]["output_overflowed"], false);
        assert!(record.get("environment_fingerprint").is_none());
    }

    let verify = run_verify(&subject, &output);
    assert!(
        verify.status.success(),
        "verification failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    fs::remove_file(output.join("logs/cold-build-01.log"))
        .expect("diagnostic log must be removable");
    let verify_without_log = run_verify(&subject, &output);
    assert!(
        verify_without_log.status.success(),
        "diagnostic logs must not be integrity evidence: {}",
        String::from_utf8_lossy(&verify_without_log.stderr)
    );
    assert_scratch_removed(&output);
}

#[test]
fn collection_preserves_explicit_compiler_environment() {
    let temporary = TemporaryDirectory::new("compiler-environment");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    fs::write(
        subject.join("reference-game/build"),
        "import os\nprint(f\"compiler-lib={os.environ.get('LIB', '')}\")\nprint(f\"compiler-include={os.environ.get('INCLUDE', '')}\")\n",
    )
    .expect("fake build command must be writable");
    commit_all(&subject, "record compiler environment");
    let output = temporary.path().join("measurement-output");
    let library_path = temporary.path().join("compiler-library");
    let include_path = temporary.path().join("compiler-include");

    let result = run_collection(
        &subject,
        &output,
        "10",
        &[("LIB", &library_path), ("INCLUDE", &include_path)],
    );

    assert!(
        result.status.success(),
        "collection failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let log = fs::read_to_string(output.join("logs/cold-build-01.log"))
        .expect("cold build log must exist");
    assert!(log.contains(&format!("compiler-lib={}", library_path.display())));
    assert!(log.contains(&format!("compiler-include={}", include_path.display())));
}

#[test]
fn collection_bounds_total_diagnostic_log_bytes() {
    let temporary = TemporaryDirectory::new("diagnostic-log-budget");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    set_diagnostic_log_budget(&subject, 256);
    fs::write(subject.join("reference-game/build"), "print('x' * 1024)\n")
        .expect("fake build command must be writable");
    commit_all(&subject, "exercise diagnostic log budget");
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output, "10", &[]);

    assert!(
        result.status.success(),
        "collection failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let retained = fs::read_dir(output.join("logs"))
        .expect("diagnostic log directory must exist")
        .map(|entry| {
            entry
                .expect("diagnostic log entry must be readable")
                .metadata()
                .expect("diagnostic log metadata must be readable")
                .len()
        })
        .sum::<u64>();
    assert!(retained <= 256, "retained {retained} diagnostic bytes");
}

#[test]
fn failed_command_retains_structured_outcome_and_verifiable_transport() {
    let temporary = TemporaryDirectory::new("build-failure");
    let subject = create_subject(
        temporary.path(),
        "subject",
        CollectionMode::BuildFailure,
        None,
    );
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output, "10", &[]);

    assert!(!result.status.success());
    let manifest = read_manifest(&output);
    assert_eq!(manifest["status"], "failed");
    assert!(manifest["failure"].as_str().unwrap().contains("status 7"));
    let records = read_records(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["metric_id"], "build.cold_ns");
    assert!(records[0]["sample_value"].is_null());
    assert_eq!(records[0]["command"]["exit_code"], 7);
    assert_eq!(records[0]["command"]["timed_out"], false);
    assert_eq!(records[0]["command"]["output_overflowed"], false);
    assert!(run_verify(&subject, &output).status.success());
    assert_scratch_removed(&output);
}

#[test]
fn public_surface_timeout_and_overflow_remain_distinguishable() {
    for (label, mode, expected_field) in [
        (
            "public-timeout",
            CollectionMode::PublicSurfaceTimeout,
            "timed_out",
        ),
        (
            "public-overflow",
            CollectionMode::PublicSurfaceOverflow,
            "output_overflowed",
        ),
    ] {
        let temporary = TemporaryDirectory::new(label);
        let subject = create_subject(temporary.path(), "subject", mode, None);
        let output = temporary.path().join("measurement-output");

        let result = run_collection(&subject, &output, "1", &[]);

        assert!(!result.status.success(), "{label} must fail collection");
        let manifest = read_manifest(&output);
        assert_eq!(manifest["status"], "failed");
        assert_eq!(manifest["checks"]["public_surface"][expected_field], true);
        assert!(run_verify(&subject, &output).status.success());
        assert_scratch_removed(&output);
    }
}

#[test]
fn verifier_binds_raw_bytes_but_not_diagnostic_bytes() {
    let temporary = TemporaryDirectory::new("raw-tamper");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    let output = temporary.path().join("measurement-output");
    let result = run_collection(&subject, &output, "10", &[]);
    assert!(result.status.success());

    fs::OpenOptions::new()
        .append(true)
        .open(output.join("raw-samples.jsonl"))
        .expect("raw artifact must open")
        .write_all(b"{}\n")
        .expect("raw artifact must be mutable");

    let verify = run_verify(&subject, &output);
    assert!(!verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stderr).contains("digest does not match"));
}

#[cfg(unix)]
#[test]
fn verifier_rejects_a_symbolic_link_before_resolving_the_run_directory() {
    use std::os::unix::fs::symlink;

    let temporary = TemporaryDirectory::new("symlinked-run");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    let run = temporary.path().join("real-run");
    let link = temporary.path().join("run-link");
    fs::create_dir(&run).expect("real run directory must be creatable");
    symlink(&run, &link).expect("run symlink must be creatable");

    let verify = run_verify(&subject, &link);

    assert!(!verify.status.success());
    assert!(String::from_utf8_lossy(&verify.stderr).contains("must not be a symbolic link"));
}

#[test]
fn dirty_subject_and_subject_owned_output_reject_before_collection() {
    let temporary = TemporaryDirectory::new("preflight-refusal");
    let subject = create_subject(temporary.path(), "subject", CollectionMode::Success, None);
    fs::write(subject.join("dirty.txt"), "dirty\n").expect("dirty marker must be writable");
    let dirty_output = temporary.path().join("dirty-output");
    let dirty = run_collection(&subject, &dirty_output, "10", &[]);
    assert!(!dirty.status.success());
    assert!(!dirty_output.exists());

    fs::remove_file(subject.join("dirty.txt")).expect("dirty marker must be removable");
    let nested_output = subject.join("measurement-output");
    let nested = run_collection(&subject, &nested_output, "10", &[]);
    assert!(!nested.status.success());
    assert!(!nested_output.exists());
}

#[test]
fn timeout_retires_the_owned_execution_group() {
    let temporary = TemporaryDirectory::new("process-group");
    let sentinel = temporary.path().join("orphan-sentinel");
    let subject = create_subject(
        temporary.path(),
        "subject",
        CollectionMode::HangingBuild,
        Some(&sentinel),
    );
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output, "0.5", &[]);
    assert!(!result.status.success());
    thread::sleep(Duration::from_secs(3));
    assert!(
        !sentinel.exists(),
        "the Windows job or original POSIX process group must retire its child"
    );
    let records = read_records(&output);
    assert_eq!(records[0]["command"]["timed_out"], true);
    assert_scratch_removed(&output);
}

#[test]
fn invalid_timeouts_reject_before_creating_output() {
    for value in ["nan", "inf", "0", "-1"] {
        let temporary = TemporaryDirectory::new("invalid-timeout");
        let output = temporary.path().join("measurement-output");
        let result = run_helper(
            &repository_helper(),
            &[
                Path::new("collect"),
                Path::new("--subject"),
                temporary.path(),
                Path::new("--output"),
                &output,
                Path::new("--command-timeout-seconds"),
                Path::new(value),
            ],
            &[],
        );
        assert!(!result.status.success(), "{value} must reject");
        assert!(!output.exists());
    }
}

#[test]
fn helper_cli_is_stdlib_only_and_has_no_plan_command() {
    let helper = fs::read_to_string(repository_helper()).expect("helper must be readable");
    for forbidden in ["import requests", "import psutil"] {
        assert!(
            !helper.contains(forbidden),
            "helper must remain Python-stdlib-only: {forbidden}"
        );
    }

    let help = run_helper(&repository_helper(), &[Path::new("--help")], &[]);
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(stdout.contains("collect"));
    assert!(stdout.contains("verify"));
    assert!(!stdout.contains("plan"));
}
