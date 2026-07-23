use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const PLAN_SCHEMA: &str = "nara.reference-game.first-playable-plan-v1";
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

fn run_helper_strings(arguments: &[&str]) -> Output {
    let mut command = Command::new("python");
    command.arg("-B").arg(helper_path()).args(arguments);
    run(&mut command)
}

fn git_status(subject: &Path) -> String {
    String::from_utf8(run_git(subject, &["status", "--porcelain=v1"]).stdout)
        .expect("git status must be UTF-8")
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
fn helper_is_stdlib_only_and_exposes_no_result_claim_before_collection() {
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
    for required in [
        "prepared_not_executed",
        "not_evaluated",
        "render.packet.clone_bytes",
        "git status --porcelain=v1 -z",
        "key.upper().startswith(\"GIT_\")",
        "reference-game-first-playable.json",
        "minimum_samples",
    ] {
        assert!(
            source.contains(required),
            "measurement helper must retain `{required}`"
        );
    }

    let help = run_helper_strings(&["--help"]);
    assert!(help.status.success(), "helper help must be available");
    let help = String::from_utf8(help.stdout).expect("helper help must be UTF-8");
    assert!(help.contains("does not evaluate"));
}
