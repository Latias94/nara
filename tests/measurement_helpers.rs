use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PLAN_SCHEMA: &str = "nara.reference-game.first-playable-plan-v1";
const RUN_SCHEMA: &str = "nara.reference-game.first-playable-local-run-v2";
const METRIC_CATALOG_RELATIVE_PATH: &str =
    "docs/benchmarks/data/protocol/v1/reference-game-first-playable.json";

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
            "test cleanup must remain inside its own system temporary directory: {candidate:?}"
        );
        fs::remove_dir_all(candidate).expect("test temporary directory must be removable");
    }
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn helper_path() -> PathBuf {
    repository_root().join("reference-game/tools/measure_first_playable.py")
}

fn run(command: &mut Command) -> Output {
    command
        .output()
        .expect("test command must start successfully")
}

fn run_git(directory: &Path, arguments: &[&str]) -> Output {
    let mut command = Command::new("git");
    command.current_dir(directory).args(arguments);
    let output = run(&mut command);
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn create_subject(root: &Path, label: &str) -> PathBuf {
    let subject = root.join(label);
    fs::create_dir(&subject).expect("fixture repository directory must be creatable");
    run_git(&subject, &["init", "--quiet"]);
    run_git(&subject, &["config", "user.email", "tests@nara.invalid"]);
    run_git(&subject, &["config", "user.name", "Nara Measurement Tests"]);

    for relative in [
        "Cargo.toml",
        "reference-game/Cargo.toml",
        "reference-game/Cargo.lock",
        "reference-game/src/bin/headless.rs",
        "reference-game/src/bin/desktop.rs",
        "reference-game/src/bin/desktop_render_probe.rs",
        "reference-game/src/systems.rs",
        "reference-game/scenes/startup.scene.json",
        "reference-game/tests/public_surface.rs",
    ] {
        let path = subject.join(relative);
        fs::create_dir_all(path.parent().expect("fixture file must have a parent"))
            .expect("fixture parent directories must be creatable");
        fs::write(&path, format!("fixture: {relative}\n"))
            .expect("fixture source file must be writable");
    }
    let fixture_catalog = subject.join(METRIC_CATALOG_RELATIVE_PATH);
    fs::create_dir_all(
        fixture_catalog
            .parent()
            .expect("fixture metric catalog must have a parent"),
    )
    .expect("fixture metric catalog parent must be creatable");
    fs::copy(
        repository_root().join(METRIC_CATALOG_RELATIVE_PATH),
        &fixture_catalog,
    )
    .expect("fixture must retain the committed metric catalog");

    run_git(&subject, &["add", "."]);
    run_git(
        &subject,
        &["commit", "--quiet", "-m", "measurement fixture"],
    );
    subject
}

#[derive(Clone, Copy)]
enum CollectionFailure {
    None,
    Build,
    Headless,
    NonTerminal,
    InvalidSummary,
    PublicSurface,
}

fn create_collection_subject(root: &Path, label: &str, failure: CollectionFailure) -> PathBuf {
    let subject = create_subject(root, label);
    for relative in [
        "reference-game/src/systems.rs",
        "reference-game/scenes/startup.scene.json",
    ] {
        fs::copy(repository_root().join(relative), subject.join(relative))
            .expect("collection fixture must copy the current product source");
    }

    let reference_game = subject.join("reference-game");
    fs::write(reference_game.join("fetch"), "raise SystemExit(0)\n")
        .expect("fake Cargo fetch command must be writable");
    fs::write(
        reference_game.join("test"),
        r#"from pathlib import Path
import sys

if Path("fail-public-surface").exists():
    print("injected public-surface failure")
    raise SystemExit(11)
raise SystemExit(0)
"#,
    )
    .expect("fake Cargo test command must be writable");
    fs::write(
        reference_game.join("build"),
        r#"from pathlib import Path
import sys

if Path("fail-build").exists():
    print("injected build failure")
    raise SystemExit(7)
raise SystemExit(0)
"#,
    )
    .expect("fake Cargo build command must be writable");
    fs::write(
        reference_game.join("run"),
        r#"import json
from pathlib import Path
import sys

if Path("fail-headless").exists():
    print("injected headless failure")
    raise SystemExit(9)

scene = Path("scenes/startup.scene.json").read_text(encoding="utf-8")
hit_points = 21 if '"value": 21' in scene else 20
print(json.dumps({
    "schema": "nara-reference-game.wave-summary-v1",
    "outcome": "running" if Path("non-terminal").exists() else "completed",
    "tick": -1 if Path("invalid-summary").exists() else 49,
    "score": 300,
    "player_hit_points": hit_points,
    "enemies_remaining": 0,
    "projectiles_remaining": 4,
}, separators=(",", ":")))
"#,
    )
    .expect("fake Cargo run command must be writable");
    match failure {
        CollectionFailure::None => {}
        CollectionFailure::Build => {
            fs::write(reference_game.join("fail-build"), "fail\n")
                .expect("fake Cargo failure marker must be writable");
        }
        CollectionFailure::Headless => {
            fs::write(reference_game.join("fail-headless"), "fail\n")
                .expect("fake Cargo failure marker must be writable");
        }
        CollectionFailure::NonTerminal => {
            fs::write(reference_game.join("non-terminal"), "fail\n")
                .expect("fake non-terminal marker must be writable");
        }
        CollectionFailure::InvalidSummary => {
            fs::write(reference_game.join("invalid-summary"), "fail\n")
                .expect("fake invalid-summary marker must be writable");
        }
        CollectionFailure::PublicSurface => {
            fs::write(reference_game.join("fail-public-surface"), "fail\n")
                .expect("fake public-surface marker must be writable");
        }
    }

    run_git(&subject, &["add", "."]);
    run_git(&subject, &["commit", "--quiet", "-m", "collection fixture"]);
    subject
}

fn create_hanging_collection_subject(root: &Path, label: &str, sentinel: &Path) -> PathBuf {
    let subject = create_collection_subject(root, label, CollectionFailure::None);
    let sentinel_literal = serde_json::to_string(&sentinel.to_string_lossy())
        .expect("sentinel path must encode as a Python string");
    let child = format!(
        "from pathlib import Path\nimport time\ntime.sleep(2.0)\nPath({sentinel_literal}).write_text('orphan', encoding='utf-8')\n"
    );
    let child_literal =
        serde_json::to_string(&child).expect("child source must encode as a Python string");
    let build = format!(
        "import subprocess\nimport sys\nimport time\nsubprocess.Popen([sys.executable, '-c', {child_literal}])\ntime.sleep(30)\n"
    );
    fs::write(subject.join("reference-game/build"), build)
        .expect("hanging fake Cargo build must be writable");
    run_git(&subject, &["add", "reference-game/build"]);
    run_git(
        &subject,
        &["commit", "--quiet", "-m", "hang fake Cargo build"],
    );
    subject
}

fn create_escaping_collection_subject(root: &Path, label: &str, sentinel: &Path) -> PathBuf {
    let subject = create_collection_subject(root, label, CollectionFailure::None);
    let sentinel_literal = serde_json::to_string(&sentinel.to_string_lossy())
        .expect("sentinel path must encode as a Python string");
    let child = format!(
        "from pathlib import Path\nimport time\ntime.sleep(2.0)\nPath({sentinel_literal}).write_text('orphan', encoding='utf-8')\n"
    );
    let child_literal =
        serde_json::to_string(&child).expect("child source must encode as a Python string");
    let build = format!(
        "import subprocess\nimport sys\nsubprocess.Popen([sys.executable, '-c', {child_literal}])\n"
    );
    fs::write(subject.join("reference-game/build"), build)
        .expect("escaping fake Cargo build must be writable");
    run_git(&subject, &["add", "reference-game/build"]);
    run_git(
        &subject,
        &["commit", "--quiet", "-m", "escape fake Cargo build"],
    );
    subject
}

fn create_silent_child_failure_subject(root: &Path, label: &str, sentinel: &Path) -> PathBuf {
    let subject = create_collection_subject(root, label, CollectionFailure::None);
    let sentinel_literal = serde_json::to_string(&sentinel.to_string_lossy())
        .expect("sentinel path must encode as a Python string");
    let child = format!(
        "from pathlib import Path\nimport time\ntime.sleep(2.0)\nPath({sentinel_literal}).write_text('orphan', encoding='utf-8')\n"
    );
    let child_literal =
        serde_json::to_string(&child).expect("child source must encode as a Python string");
    let build = format!(
        "import subprocess\nimport sys\nsubprocess.Popen([sys.executable, '-c', {child_literal}], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\nraise SystemExit(7)\n"
    );
    fs::write(subject.join("reference-game/build"), build)
        .expect("silent-child fake Cargo build must be writable");
    run_git(&subject, &["add", "reference-game/build"]);
    run_git(
        &subject,
        &[
            "commit",
            "--quiet",
            "-m",
            "spawn silent child before failure",
        ],
    );
    subject
}

fn run_helper(arguments: &[&Path]) -> Output {
    run_helper_with_environment(arguments, &[])
}

fn run_helper_with_environment(arguments: &[&Path], environment: &[(&str, &Path)]) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(helper_path());
    for argument in arguments {
        command.arg(argument);
    }
    for &(key, value) in environment {
        command.env(key, value);
    }
    run(&mut command)
}

fn git_status(subject: &Path) -> String {
    String::from_utf8(run_git(subject, &["status", "--porcelain=v1"]).stdout)
        .expect("git status must be UTF-8")
}

fn run_collection(subject: &Path, output: &Path) -> Output {
    run_collection_with_timeout(subject, output, "10")
}

fn run_collection_with_timeout(subject: &Path, output: &Path, timeout: &str) -> Output {
    run_helper(&[
        Path::new("collect"),
        Path::new("--subject"),
        subject,
        Path::new("--output"),
        output,
        Path::new("--cargo"),
        Path::new("python"),
        Path::new("--command-timeout-seconds"),
        Path::new(timeout),
    ])
}

fn assert_collection_scratch_removed(output: &Path) {
    for name in ["worktree", "target", "cargo-home", "home", "temp"] {
        assert!(
            !output.join(name).exists(),
            "collection scratch `{name}` must not survive"
        );
    }
}

#[test]
fn plan_uses_a_clean_isolated_subject_without_recording_a_result() {
    let temporary = TemporaryDirectory::new("clean-plan");
    let subject = create_subject(temporary.path(), "subject");
    let output = temporary.path().join("measurement-output");
    let command = Path::new("plan");
    let subject_flag = Path::new("--subject");
    let output_flag = Path::new("--output");

    let result = run_helper(&[command, subject_flag, &subject, output_flag, &output]);

    assert!(
        result.status.success(),
        "planning failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty(), "the plan must be file-backed");
    assert_eq!(
        git_status(&subject),
        "",
        "planning must not dirty its subject"
    );

    let plan_path = output.join("measurement-plan.json");
    let plan: Value = serde_json::from_slice(
        &fs::read(&plan_path).expect("the plan must be written outside the subject"),
    )
    .expect("the plan must be valid JSON");
    let rendered = serde_json::to_string(&plan).expect("the plan must be serializable");

    assert_eq!(plan["schema"], PLAN_SCHEMA);
    assert_eq!(plan["status"], "prepared_not_executed");
    assert_eq!(plan["decision"], "not_evaluated");
    assert!(
        plan.get("sample_count").is_none(),
        "the plan must use each canonical metric's minimum instead of one global sample count"
    );
    assert!(
        !rendered.contains(&subject.to_string_lossy().to_string()),
        "the local plan must not leak the subject's absolute path"
    );
    for required_step in [
        "clean_headless_wave",
        "data_edit_reload",
        "structural_rust_edit",
        "desktop_manual_playthrough",
        "public_production_coverage",
        "build_timings",
        "body_edit_reload",
    ] {
        assert!(
            plan["steps"]
                .as_array()
                .expect("plan steps must be an array")
                .iter()
                .any(|step| step["id"] == required_step),
            "the plan must contain {required_step}"
        );
    }
    let desktop_command = plan["steps"]
        .as_array()
        .expect("plan steps must be an array")
        .iter()
        .find(|step| step["id"] == "desktop_manual_playthrough")
        .and_then(|step| step["command"].as_array())
        .expect("the desktop plan must carry its public Cargo command");
    assert!(
        desktop_command
            .iter()
            .any(|argument| argument == "--features")
            && desktop_command.iter().any(|argument| argument == "desktop"),
        "the desktop plan must enable the feature required by its public binary"
    );
    for blocked_workflow in [
        "module_addition",
        "window_slot_configuration",
        "desktop_pressure_telemetry",
    ] {
        assert!(
            plan["blocked_workflows"]
                .as_array()
                .expect("blocked workflows must be an array")
                .iter()
                .any(|workflow| workflow["id"] == blocked_workflow),
            "the plan must name the unresolved {blocked_workflow} workflow"
        );
    }
    let metric_requirements = plan["metric_catalog"]["requirements"]
        .as_array()
        .expect("metric requirements must be an array");
    for (metric_id, minimum_samples) in [
        ("build.cold_ns", 5),
        ("iteration.body.p95_ns", 10),
        ("frame.p99_ns", 1000),
        ("gameplay.desktop_playable_success", 1),
    ] {
        assert!(
            metric_requirements.iter().any(|metric| {
                metric["id"] == metric_id && metric["minimum_samples"] == minimum_samples
            }),
            "the plan must retain the canonical requirement for {metric_id}"
        );
    }
    for unavailable in [
        "frame.p99_ns",
        "runtime.memory_bytes",
        "render.packet.instance_count",
        "render.packet.clone_bytes",
    ] {
        assert!(
            plan["unavailable_measurements"]
                .as_array()
                .expect("unavailable measurements must be an array")
                .iter()
                .any(|measurement| measurement["id"] == unavailable),
            "the plan must honestly name the missing {unavailable} collector"
        );
    }
}

#[test]
fn plan_refuses_dirty_subjects_before_creating_output() {
    let temporary = TemporaryDirectory::new("dirty-subject");
    let subject = create_subject(temporary.path(), "subject");
    fs::write(
        subject.join("reference-game/scenes/startup.scene.json"),
        "dirty fixture\n",
    )
    .expect("fixture must become dirty");
    let output = temporary.path().join("measurement-output");

    let result = run_helper(&[
        Path::new("plan"),
        Path::new("--subject"),
        &subject,
        Path::new("--output"),
        &output,
    ]);

    assert!(!result.status.success(), "a dirty subject must reject");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("clean"),
        "the error must name the clean-subject requirement"
    );
    assert!(
        !output.exists(),
        "rejection must happen before the helper creates output"
    );
}

#[test]
fn plan_refuses_output_inside_the_measurement_subject() {
    let temporary = TemporaryDirectory::new("inside-subject");
    let subject = create_subject(temporary.path(), "subject");
    let output = subject.join("measurement-output");

    let result = run_helper(&[
        Path::new("plan"),
        Path::new("--subject"),
        &subject,
        Path::new("--output"),
        &output,
    ]);

    assert!(
        !result.status.success(),
        "inside-subject output must reject"
    );
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("outside"),
        "the error must name the output-boundary requirement"
    );
    assert_eq!(
        git_status(&subject),
        "",
        "an output-boundary rejection must not dirty the subject"
    );
    assert!(
        !output.exists(),
        "the helper must not create an output directory inside the subject"
    );
}

#[test]
fn plan_refuses_a_clean_subject_with_a_metric_catalog_workflow_mismatch() {
    let temporary = TemporaryDirectory::new("metric-catalog-mismatch");
    let subject = create_subject(temporary.path(), "subject");
    let catalog_path = subject.join(METRIC_CATALOG_RELATIVE_PATH);
    let mut catalog: Value = serde_json::from_slice(
        &fs::read(&catalog_path).expect("fixture metric catalog must be readable"),
    )
    .expect("fixture metric catalog must be valid JSON");
    catalog["metrics"]
        .as_array_mut()
        .expect("fixture metric catalog must contain metrics")
        .retain(|metric| metric["id"] != "frame.p99_ns");
    fs::write(
        &catalog_path,
        serde_json::to_vec_pretty(&catalog).expect("fixture metric catalog must serialize"),
    )
    .expect("fixture metric catalog must be writable");
    run_git(&subject, &["add", METRIC_CATALOG_RELATIVE_PATH]);
    run_git(
        &subject,
        &["commit", "--quiet", "-m", "remove required metric"],
    );
    let output = temporary.path().join("measurement-output");

    let result = run_helper(&[
        Path::new("plan"),
        Path::new("--subject"),
        &subject,
        Path::new("--output"),
        &output,
    ]);

    assert!(!result.status.success(), "a mismatched catalog must reject");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("workflow disagree"),
        "the error must name the catalog/workflow boundary"
    );
    assert!(
        !output.exists(),
        "a catalog mismatch must reject before creating an output directory"
    );
}

#[test]
fn plan_ignores_git_environment_overrides_when_proving_its_subject() {
    let temporary = TemporaryDirectory::new("git-environment");
    let subject = create_subject(temporary.path(), "subject");
    let unrelated = create_subject(temporary.path(), "unrelated");
    fs::write(
        unrelated.join("unrelated-marker.txt"),
        "unrelated revision\n",
    )
    .expect("unrelated fixture must be writable");
    run_git(&unrelated, &["add", "unrelated-marker.txt"]);
    run_git(
        &unrelated,
        &["commit", "--quiet", "-m", "unrelated revision"],
    );
    let expected_revision = String::from_utf8(run_git(&subject, &["rev-parse", "HEAD"]).stdout)
        .expect("subject revision must be UTF-8");
    let output = temporary.path().join("measurement-output");
    let override_index = temporary.path().join("override-index");
    let override_git_directory = unrelated.join(".git");

    let result = run_helper_with_environment(
        &[
            Path::new("plan"),
            Path::new("--subject"),
            &subject,
            Path::new("--output"),
            &output,
        ],
        &[
            ("GIT_DIR", override_git_directory.as_path()),
            ("GIT_INDEX_FILE", override_index.as_path()),
        ],
    );

    assert!(
        result.status.success(),
        "the helper must prove the supplied subject, not inherited Git state: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let plan: Value = serde_json::from_slice(
        &fs::read(output.join("measurement-plan.json"))
            .expect("the plan must be written after the real subject is proven"),
    )
    .expect("the plan must be valid JSON");
    assert_eq!(
        plan["source"]["revision"],
        expected_revision.trim(),
        "the plan must bind the real subject revision"
    );
    assert_eq!(
        git_status(&subject),
        "",
        "Git inspection must not dirty the supplied subject"
    );
}

#[test]
fn collect_and_verify_use_an_isolated_worktree_and_restore_every_edit() {
    let temporary = TemporaryDirectory::new("collect-success");
    let subject = create_collection_subject(temporary.path(), "subject", CollectionFailure::None);
    let revision = String::from_utf8(run_git(&subject, &["rev-parse", "HEAD"]).stdout)
        .expect("collection revision must be UTF-8");
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output);

    assert!(
        result.status.success(),
        "collection failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty(), "collection must be file-backed");
    assert_eq!(
        git_status(&subject),
        "",
        "collection must not dirty its subject"
    );
    assert_eq!(
        String::from_utf8(run_git(&subject, &["rev-parse", "HEAD"]).stdout)
            .expect("post-collection revision must be UTF-8"),
        revision,
        "collection must not advance the source revision"
    );
    let worktrees =
        String::from_utf8(run_git(&subject, &["worktree", "list", "--porcelain"]).stdout)
            .expect("worktree list must be UTF-8");
    assert_eq!(
        worktrees.matches("worktree ").count(),
        1,
        "the collector must unregister its detached worktree"
    );

    let manifest: Value = serde_json::from_slice(
        &fs::read(output.join("run-manifest.json")).expect("run manifest must exist"),
    )
    .expect("run manifest must be JSON");
    assert_eq!(manifest["schema"], RUN_SCHEMA);
    assert_eq!(manifest["format_version"], 2);
    assert_eq!(manifest["status"], "automatic_slice_complete");
    assert_eq!(manifest["decision"], "not_evaluated");
    assert_eq!(manifest["source_revision"], revision.trim());
    assert!(
        manifest["missing_metrics"]
            .as_array()
            .expect("missing metrics must be an array")
            .iter()
            .any(|metric| metric["id"] == "gameplay.desktop_playable_success"),
        "automatic collection must preserve the manual desktop gap"
    );

    let raw = fs::read_to_string(output.join("raw-samples.jsonl")).expect("raw samples must exist");
    let records = raw
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("raw sample must be JSON"))
        .collect::<Vec<_>>();
    let metric_ids = [
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
    let catalog: Value = serde_json::from_slice(
        &fs::read(repository_root().join(METRIC_CATALOG_RELATIVE_PATH))
            .expect("metric catalog must be readable"),
    )
    .expect("metric catalog must be JSON");
    let protocol_metrics = catalog["metrics"]
        .as_array()
        .expect("protocol metrics must be an array");
    let expected_samples = metric_ids
        .iter()
        .map(|metric| {
            protocol_metrics
                .iter()
                .find(|requirement| requirement["id"] == *metric)
                .unwrap_or_else(|| panic!("protocol requirement {metric} must exist"))
        })
        .map(|requirement| {
            requirement["minimum_samples"]
                .as_u64()
                .expect("sample floor must be an unsigned integer") as usize
        })
        .sum::<usize>();
    assert_eq!(manifest["raw_sample_count"], expected_samples);
    assert_eq!(records.len(), expected_samples);

    for metric in metric_ids {
        let requirement = protocol_metrics
            .iter()
            .find(|requirement| requirement["id"] == metric)
            .unwrap_or_else(|| panic!("protocol requirement {metric} must exist"));
        let metric_records = records
            .iter()
            .filter(|record| record["metric_id"] == metric)
            .collect::<Vec<_>>();
        assert_eq!(
            metric_records.len(),
            requirement["minimum_samples"]
                .as_u64()
                .expect("sample floor must be an unsigned integer") as usize,
            "collection must retain the complete {metric} population"
        );
        for record in metric_records {
            assert_eq!(
                record["value_unit"], requirement["value_kind"],
                "{metric} unit must match"
            );
            assert_eq!(
                record["population"], requirement["population"],
                "{metric} population must match"
            );
            assert_eq!(
                record["mechanism"], requirement["method_id"],
                "{metric} method must match"
            );
            assert_eq!(
                record["start_boundary"], requirement["start_boundary_id"],
                "{metric} start boundary must match"
            );
            assert_eq!(
                record["end_boundary"], requirement["end_boundary_id"],
                "{metric} end boundary must match"
            );
        }
    }

    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "fresh run verification failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn failed_collection_preserves_a_bounded_verifiable_sample() {
    let temporary = TemporaryDirectory::new("collect-failure");
    let subject = create_collection_subject(temporary.path(), "subject", CollectionFailure::Build);
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output);

    assert!(
        !result.status.success(),
        "the injected build must fail collection"
    );
    assert_eq!(
        git_status(&subject),
        "",
        "failed collection must leave the subject clean"
    );
    let manifest: Value = serde_json::from_slice(
        &fs::read(output.join("run-manifest.json"))
            .expect("failed collection must retain a manifest"),
    )
    .expect("failed collection manifest must be JSON");
    assert_eq!(manifest["schema"], RUN_SCHEMA);
    assert_eq!(manifest["format_version"], 2);
    assert_eq!(manifest["status"], "collection_failed");
    assert_eq!(manifest["raw_sample_count"], 1);

    let raw = fs::read_to_string(output.join("raw-samples.jsonl"))
        .expect("failed collection must retain raw samples");
    let record: Value = serde_json::from_str(raw.trim()).expect("failure sample must be JSON");
    assert_eq!(record["metric_id"], "build.cold_ns");
    assert_eq!(record["exit_status"], 7);
    assert!(record["sample_value"].is_null());
    let failure_log = record["command_output_reference"]
        .as_str()
        .expect("failure sample must reference its bounded log");
    assert!(
        output.join(failure_log).is_file(),
        "failure output must remain inspectable"
    );

    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "a truthful failed run must remain structurally verifiable: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn clean_headless_failure_preserves_journey_and_hard_stop_samples() {
    let temporary = TemporaryDirectory::new("headless-failure");
    let subject =
        create_collection_subject(temporary.path(), "subject", CollectionFailure::Headless);
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output);

    assert!(
        !result.status.success(),
        "the injected headless run must fail"
    );
    let raw = fs::read_to_string(output.join("raw-samples.jsonl"))
        .expect("failed collection must retain raw samples");
    let records = raw
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("raw sample must be JSON"))
        .collect::<Vec<_>>();
    assert_eq!(
        records.len(),
        3,
        "cold build, journey, and hard-stop outcomes must all remain"
    );
    for metric in [
        "journey.clean_to_headless_wave_ns",
        "gameplay.headless_wave_success",
    ] {
        let record = records
            .iter()
            .find(|record| record["metric_id"] == metric)
            .unwrap_or_else(|| panic!("failed collection must retain {metric}"));
        assert_eq!(record["exit_status"], 9);
        assert!(record["sample_value"].is_null());
        assert!(record["command_output_reference"].is_string());
    }
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "a truthful headless failure must remain verifiable: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn invalid_headless_summaries_cannot_complete_collection() {
    let temporary = TemporaryDirectory::new("invalid-headless-summary");
    for (failure, label, expected_error) in [
        (
            CollectionFailure::NonTerminal,
            "non-terminal",
            "completed reference wave",
        ),
        (
            CollectionFailure::InvalidSummary,
            "invalid-state",
            "valid completed state",
        ),
    ] {
        let subject = create_collection_subject(temporary.path(), label, failure);
        let output = temporary.path().join(format!("{label}-output"));

        let result = run_collection(&subject, &output);

        assert!(!result.status.success(), "{label} must not complete U9");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains(expected_error),
            "the failure must identify the invalid terminal result: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let manifest: Value = serde_json::from_slice(
            &fs::read(output.join("run-manifest.json"))
                .expect("semantic failure must retain a manifest"),
        )
        .expect("semantic failure manifest must be JSON");
        assert_eq!(manifest["status"], "collection_failed");
        assert_eq!(manifest["observed_sample_counts"]["build.cold_ns"], 1);
        assert!(
            manifest["observed_sample_counts"]
                .get("gameplay.headless_wave_success")
                .is_none()
        );
        let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
        assert!(
            verify.status.success(),
            "the truthful semantic failure must remain verifiable: {}",
            String::from_utf8_lossy(&verify.stderr)
        );
        assert_collection_scratch_removed(&output);
    }
}

#[test]
fn public_surface_failure_retains_its_check_and_log() {
    let temporary = TemporaryDirectory::new("public-surface-failure");
    let subject = create_collection_subject(
        temporary.path(),
        "subject",
        CollectionFailure::PublicSurface,
    );
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output);

    assert!(
        !result.status.success(),
        "the injected check must fail collection"
    );
    let manifest_path = output.join("run-manifest.json");
    let mut manifest: Value = serde_json::from_slice(
        &fs::read(&manifest_path).expect("failed collection must retain its manifest"),
    )
    .expect("failed collection manifest must be JSON");
    assert_eq!(manifest["status"], "collection_failed");
    assert_eq!(manifest["checks"]["public_surface"]["exit_status"], 11);
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "the truthful check failure must verify: {}",
        String::from_utf8_lossy(&verify.stderr)
    );

    manifest["checks"] = Value::Object(Default::default());
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("mutated manifest must encode"),
    )
    .expect("mutated manifest must be writable");
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "a public-surface failure cannot discard its check evidence"
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn verifier_rejects_protocol_mismatch_and_empty_completion_forgery() {
    let temporary = TemporaryDirectory::new("fresh-run-forgery");
    let subject = create_collection_subject(temporary.path(), "subject", CollectionFailure::None);
    let output = temporary.path().join("measurement-output");
    let result = run_collection(&subject, &output);
    assert!(
        result.status.success(),
        "fixture collection failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let plan_path = output.join("measurement-plan.json");
    let raw_path = output.join("raw-samples.jsonl");
    let manifest_path = output.join("run-manifest.json");
    let original_plan = fs::read(&plan_path).expect("plan must be readable");
    let original_raw = fs::read(&raw_path).expect("raw samples must be readable");
    let original_manifest = fs::read(&manifest_path).expect("manifest must be readable");

    let protocol_mutation = r#"
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
raw_path = run / "raw-samples.jsonl"
records = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines()]
for record in records:
    if record["metric_id"] == "iteration.body.p50_ns":
        record["population"] = "cold"
        break
else:
    raise AssertionError("body metric is missing")
raw = b"".join(
    json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"
    for record in records
)
raw_path.write_bytes(raw)
manifest_path = run / "run-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["raw_samples_sha256"] = hashlib.sha256(raw).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
"#;
    let mutation = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(protocol_mutation)
        .arg(&output));
    assert!(
        mutation.status.success(),
        "protocol forgery fixture must be created: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "a raw metric cannot contradict its protocol metadata"
    );

    fs::write(&plan_path, &original_plan).expect("plan must be restorable");
    fs::write(&raw_path, &original_raw).expect("raw samples must be restorable");
    fs::write(&manifest_path, &original_manifest).expect("manifest must be restorable");
    let false_success = r#"
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
raw_path = run / "raw-samples.jsonl"
records = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines()]
for record in records:
    if record["metric_id"] == "gameplay.headless_wave_success":
        record["sample_value"] = 0
        break
else:
    raise AssertionError("headless success metric is missing")
raw = b"".join(
    json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"
    for record in records
)
raw_path.write_bytes(raw)
manifest_path = run / "run-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["raw_samples_sha256"] = hashlib.sha256(raw).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
"#;
    let mutation = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(false_success)
        .arg(&output));
    assert!(
        mutation.status.success(),
        "false-success fixture must be created: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "an explicit false value cannot satisfy the headless success metric"
    );

    fs::write(&plan_path, &original_plan).expect("plan must be restorable");
    fs::write(&raw_path, &original_raw).expect("raw samples must be restorable");
    fs::write(&manifest_path, &original_manifest).expect("manifest must be restorable");
    let stripped_edit_evidence = r#"
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
raw_path = run / "raw-samples.jsonl"
records = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines()]
for record in records:
    if record["metric_id"].startswith("iteration."):
        record["result_digest"] = None
raw = b"".join(
    json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"
    for record in records
)
raw_path.write_bytes(raw)
manifest_path = run / "run-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["raw_samples_sha256"] = hashlib.sha256(raw).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
"#;
    let mutation = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(stripped_edit_evidence)
        .arg(&output));
    assert!(
        mutation.status.success(),
        "stripped-edit fixture must be created: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "a completed edit population must retain its semantic result binding"
    );

    fs::write(&plan_path, &original_plan).expect("plan must be restorable");
    fs::write(&raw_path, &original_raw).expect("raw samples must be restorable");
    fs::write(&manifest_path, &original_manifest).expect("manifest must be restorable");
    let false_data_result = r#"
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
raw_path = run / "raw-samples.jsonl"
records = [json.loads(line) for line in raw_path.read_text(encoding="utf-8").splitlines()]
for record in records:
    if record["metric_id"].startswith("iteration.data.") and record["sample_index"] % 2 == 1:
        record["result_digest"] = "f" * 64
raw = b"".join(
    json.dumps(record, sort_keys=True, separators=(",", ":")).encode("utf-8") + b"\n"
    for record in records
)
raw_path.write_bytes(raw)
manifest_path = run / "run-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["raw_samples_sha256"] = hashlib.sha256(raw).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
"#;
    let mutation = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(false_data_result)
        .arg(&output));
    assert!(
        mutation.status.success(),
        "false data-result fixture must be created: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "a data edit must produce its exact expected terminal result"
    );

    fs::write(&plan_path, &original_plan).expect("plan must be restorable");
    fs::write(&raw_path, &original_raw).expect("raw samples must be restorable");
    fs::write(&manifest_path, &original_manifest).expect("manifest must be restorable");
    let empty_completion = r#"
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
plan_path = run / "measurement-plan.json"
plan = json.loads(plan_path.read_text(encoding="utf-8"))
for requirement in plan["metric_catalog"]["requirements"]:
    requirement["minimum_samples"] = 0
plan_path.write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n", encoding="utf-8")
raw = b""
(run / "raw-samples.jsonl").write_bytes(raw)
manifest_path = run / "run-manifest.json"
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["environment"] = {}
manifest["raw_sample_count"] = 0
manifest["raw_samples_sha256"] = hashlib.sha256(raw).hexdigest()
manifest["base_terminal_summary"] = None
manifest["base_terminal_summary_digest"] = None
manifest["observed_sample_counts"] = {}
manifest["missing_metrics"] = []
manifest["checks"] = {}
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
"#;
    let mutation = run(Command::new("python")
        .arg("-B")
        .arg("-c")
        .arg(empty_completion)
        .arg(&output));
    assert!(
        mutation.status.success(),
        "empty completion fixture must be created: {}",
        String::from_utf8_lossy(&mutation.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        !verify.status.success(),
        "zero floors and an empty environment cannot prove completion"
    );
}

#[test]
fn non_finite_command_timeouts_reject_before_output_creation() {
    let temporary = TemporaryDirectory::new("non-finite-timeout");
    let missing_subject = temporary.path().join("missing-subject");
    for value in ["nan", "inf", "-inf"] {
        let output = temporary.path().join(format!("output-{value}"));
        let timeout = PathBuf::from(format!("--command-timeout-seconds={value}"));
        let result = run_helper(&[
            Path::new("collect"),
            Path::new("--subject"),
            &missing_subject,
            Path::new("--output"),
            &output,
            &timeout,
        ]);
        assert!(!result.status.success(), "{value} must not be a deadline");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("finite"),
            "the parser must identify a non-finite deadline: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            !output.exists(),
            "invalid timeout rejection must precede output creation"
        );
    }
}

#[test]
fn command_timeout_terminates_the_complete_process_tree() {
    let temporary = TemporaryDirectory::new("process-tree-timeout");
    let sentinel = temporary.path().join("orphan-sentinel.txt");
    let subject = create_hanging_collection_subject(temporary.path(), "subject", &sentinel);
    let output = temporary.path().join("measurement-output");

    let result = run_collection_with_timeout(&subject, &output, "0.2");

    assert!(!result.status.success(), "the hanging build must time out");
    assert!(
        output.join("run-manifest.json").is_file(),
        "a command timeout must remain a verifiable collection failure: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    thread::sleep(Duration::from_millis(2_500));
    assert!(
        !sentinel.exists(),
        "the timed-out command must not leave a surviving child process: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "the timed-out run must remain structurally verifiable: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn command_timeout_owns_children_after_the_direct_parent_exits() {
    let temporary = TemporaryDirectory::new("escaped-process-timeout");
    let sentinel = temporary.path().join("escaped-sentinel.txt");
    let subject = create_escaping_collection_subject(temporary.path(), "subject", &sentinel);
    let output = temporary.path().join("measurement-output");

    let result = run_collection_with_timeout(&subject, &output, "0.2");

    assert!(
        !result.status.success(),
        "an inherited-output child must keep the bounded command incomplete"
    );
    assert!(
        output.join("run-manifest.json").is_file(),
        "the escaped-child timeout must remain a verifiable collection failure: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    thread::sleep(Duration::from_millis(2_500));
    assert!(
        !sentinel.exists(),
        "a child cannot outlive its completed direct parent: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "the escaped-child timeout must remain structurally verifiable: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn completed_commands_retire_silent_child_processes() {
    let temporary = TemporaryDirectory::new("silent-child-retirement");
    let sentinel = temporary.path().join("silent-child-sentinel.txt");
    let subject = create_silent_child_failure_subject(temporary.path(), "subject", &sentinel);
    let output = temporary.path().join("measurement-output");

    let result = run_collection(&subject, &output);

    assert!(
        !result.status.success(),
        "the injected build failure must remain visible"
    );
    thread::sleep(Duration::from_millis(2_500));
    assert!(
        !sentinel.exists(),
        "a child with detached output cannot outlive its completed parent: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let verify = run_helper(&[Path::new("verify"), Path::new("--run"), &output]);
    assert!(
        verify.status.success(),
        "the bounded build failure must remain verifiable: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    assert_collection_scratch_removed(&output);
}

#[test]
fn helper_is_stdlib_only_and_does_not_claim_a_product_result() {
    let source = fs::read_to_string(helper_path()).expect("measurement helper must exist");

    for forbidden in [
        "import requests",
        "import urllib",
        "import socket",
        "subprocess.check_call",
        "Path.home(",
        "expanduser(",
        "pip install",
    ] {
        assert!(
            !source.contains(forbidden),
            "measurement helper must not contain `{forbidden}`"
        );
    }
    let help = run_helper(&[Path::new("--help")]);
    assert!(help.status.success(), "helper help must be available");
    let help = String::from_utf8(help.stdout).expect("helper help must be UTF-8");
    let normalized_help = help.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(normalized_help.contains("does not evaluate a performance or product result"));
}
