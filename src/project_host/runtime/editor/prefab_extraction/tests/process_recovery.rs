//! Actual process interruption, public Session production and CLI-only recovery.
//!
//! Build the companion separately and set NARA_PREFAB_EXTRACTION_RECOVERY_EXE before running
//! the ignored `subprocess_` tests. Neither the parent nor its children invoke Cargo.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    process::{Command, Output, Stdio},
};

use nara_fs::FileCapability;
use nara_project::{ProjectAuthoringState, ProjectManifest};
use nara_scene::SceneCompilationLimits;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;
use crate::project_host::{
    EditorProjectOpenFailure, EditorProjectOpenFailureSource, EditorProjectOpenHost,
    EditorProjectOpenPhase, EditorProjectOpenProgress, PendingAuthoringRecoveryInspection,
    PendingAuthoringRecoveryKind, PrefabExtractionRecovery,
    PrefabExtractionRecoveryErrorKind as RecoveryError,
    PrefabExtractionRecoveryPhase as RecoveryPhase,
};

const COMPANION_ENV: &str = "NARA_PREFAB_EXTRACTION_RECOVERY_EXE";
const MODE_ENV: &str = "NARA_PREFAB_EXTRACTION_CHILD_MODE";
const ROOT_ENV: &str = "NARA_PREFAB_EXTRACTION_CHILD_ROOT";
const PROFILE_ENV: &str = "NARA_PREFAB_EXTRACTION_CHILD_PROFILE";
const CHILD_ENTRY: &str =
    "project_host::runtime::editor::prefab_extraction::tests::process_recovery::child_entry";
const ORACLE: &str = "review-oracle.json";
const PROCESS_WAIT: Duration = Duration::from_secs(90);
const PRESSURE_SOURCES: usize = 8;

type Disk = BTreeMap<PathBuf, Vec<u8>>;

/// Only a test oracle: these bytes are never fed to a decoder or used to perform recovery.
#[derive(Debug, Serialize, Deserialize)]
struct ReviewOracle {
    original: Disk,
    outputs: Disk,
    source_count: usize,
}

struct ProcessContainer(PathBuf);

impl ProcessContainer {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nara_extraction_process_{}_{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for ProcessContainer {
    fn drop(&mut self) {
        let (Ok(path), Ok(temporary)) =
            (self.0.canonicalize(), std::env::temp_dir().canonicalize())
        else {
            return;
        };
        if path.parent() == Some(temporary.as_path())
            && path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara_extraction_process_")
        {
            // This parent owns the entire container, including the intentionally leaked child's
            // Fixture. Resolve and check the exact target before recursive cleanup.
            let _ = fs::remove_dir_all(path);
        }
    }
}

struct ProcessProject {
    root: PathBuf,
    oracle: ReviewOracle,
    _container: ProcessContainer,
}

fn companion() -> OsString {
    let executable = std::env::var_os(COMPANION_ENV).unwrap_or_else(|| {
        panic!("set {COMPANION_ENV} to the separately built CLI-only recovery companion")
    });
    assert!(
        Path::new(&executable).is_file(),
        "{COMPANION_ENV} must name an existing executable"
    );
    executable
}

fn run(mut command: Command) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    // Drain both pipes while the child is alive; a diagnostic must not fill a pipe and prevent
    // the bounded child from returning. Only this test's actual child is killed on timeout.
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let deadline = Instant::now() + PROCESS_WAIT;
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break (status, false);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break (child.wait().unwrap(), true);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    };
    assert!(
        !timed_out,
        "the actual child exceeded {PROCESS_WAIT:?}: {output:?}"
    );
    output
}

fn succeeded(output: Output) -> String {
    assert!(
        output.status.success(),
        "child failed: {}; stdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8(output.stdout).unwrap()
}

fn session_child(mode: &str, root: &Path, temporary: &Path, profile: &str) -> Output {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            CHILD_ENTRY,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MODE_ENV, mode)
        .env(ROOT_ENV, root)
        .env(PROFILE_ENV, profile)
        .env("TEMP", temporary)
        .env("TMP", temporary)
        .env("TMPDIR", temporary);
    run(command)
}

fn snapshot(root: &Path) -> Disk {
    let mut directories = vec![root.to_owned()];
    let mut files = BTreeMap::new();
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                directories.push(entry.path());
            } else {
                assert!(
                    kind.is_file(),
                    "the process fixture contains only regular files"
                );
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    files
}

fn manifest(root: &Path) -> ProjectManifest {
    let deadline = Instant::now() + WAIT;
    let bytes = loop {
        match fs::read(root.join("nara.toml")) {
            Ok(bytes) => break bytes,
            Err(error)
                if cfg!(windows)
                    && matches!(error.raw_os_error(), Some(32 | 33))
                    && Instant::now() < deadline =>
            {
                // The actual Pending worker may still hold its exclusive range lock.
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("the actual manifest observation failed: {error}"),
        }
    };
    let loaded = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    loaded.manifest.unwrap()
}

fn produce_pending(container: &Path, pressure: bool) -> ! {
    let fixture = Fixture::new();
    let root = fixture.root.canonicalize().unwrap();
    let container = container.canonicalize().unwrap();
    assert!(root.starts_with(&container) && root != container);
    if pressure {
        for index in 0..PRESSURE_SOURCES {
            let path = format!("prefabs/pressure-{index}.prefab.ron");
            fs::write(
                root.join(path),
                PrefabDocument::from_ordered_roots([instance("closed", ALIAS, true)])
                    .to_ron_string()
                    .unwrap(),
            )
            .unwrap();
        }
    }
    let original = snapshot(&root);
    let mut editor = fixture.open();
    wait(&mut editor, |editor| {
        !editor.project_source_index_work_pending()
    });
    let startup = editor.workspace().active_document().unwrap();
    let selected = open(&mut editor, P, AuthoringDocumentKind::Prefab);
    let inactive = open(&mut editor, INACTIVE, AuthoringDocumentKind::Scene);
    let guard = open(&mut editor, GUARD, AuthoringDocumentKind::Prefab);
    for (document, entity, value) in [
        (startup, "left", "dirty-startup"),
        (selected, "branch", "dirty-branch"),
        (inactive, "target", "dirty-inactive"),
        (guard, "target", "dirty-guard"),
    ] {
        edit(&mut editor, document, entity, value);
        assert!(state(&editor, document).dirty);
    }
    let mut extraction = request(selected);
    extraction.save_current.push(inactive);
    let limits =
        AuthoringSourceOperationLimits::default().with_open_handles(ItemLimit::new(128).unwrap());
    let operation = editor
        .begin_prefab_extraction_with_limits(extraction, limits)
        .unwrap();
    reviewed(&mut editor, operation);
    let review = editor.prefab_extraction_review(operation).unwrap();
    let oracle = ReviewOracle {
        original,
        outputs: review
            .outputs()
            .map(|output| {
                (
                    PathBuf::from(output.project_path()),
                    output.bytes().to_vec(),
                )
            })
            .collect(),
        source_count: review.source_count(),
    };
    assert_eq!(
        oracle.source_count,
        SOURCES.len() + if pressure { PRESSURE_SOURCES } else { 0 }
    );
    assert_eq!(
        oracle.outputs.len(),
        8 + if pressure { PRESSURE_SOURCES } else { 0 }
    );
    assert!(!oracle.outputs.contains_key(Path::new(GUARD)));
    assert_eq!(
        snapshot(&root),
        oracle.original,
        "Review must not publish any fixture bytes"
    );
    // This file belongs to the parent's container, outside the project and its authenticated
    // package. Recovery cannot consume it; it only carries the actual public Review oracle.
    fs::write(container.join(ORACLE), serde_json::to_vec(&oracle).unwrap()).unwrap();
    editor.confirm_prefab_extraction(operation).unwrap();
    wait(&mut editor, |editor| {
        matches!(
            editor.prefab_extraction_view().phase,
            EditorPrefabExtractionPhase::PublishingPending
                | EditorPrefabExtractionPhase::RequiresReconcile
                | EditorPrefabExtractionPhase::Terminal
        )
    });
    assert_eq!(
        editor.prefab_extraction_view().phase,
        EditorPrefabExtractionPhase::PublishingPending
    );
    // This actual worker can publish only Pending. Do not drive another frame: its return
    // would admit the first source step. Exit intentionally skips Session and Fixture Drop.
    let deadline = Instant::now() + WAIT;
    while !matches!(
        manifest(&root).authoring.state,
        ProjectAuthoringState::Pending(_)
    ) {
        assert!(
            Instant::now() < deadline,
            "the real Pending replacement must become visible"
        );
        thread::sleep(Duration::from_millis(1));
    }
    for (path, bytes) in &oracle.original {
        if path != Path::new("nara.toml") {
            assert_eq!(
                &fs::read(root.join(path)).unwrap(),
                bytes,
                "{}",
                path.display()
            );
        }
    }
    assert!(!root.join(Q).exists() && !root.join(Q_META).exists());
    println!("\nfixture-root={}", root.display());
    println!(
        "producer pending=true sources_started=false dirty_documents=4 sources={}",
        oracle.source_count
    );
    std::io::stdout().flush().unwrap();
    std::process::exit(0);
}

fn pending_project(pressure: bool) -> ProcessProject {
    let _ = companion();
    let container = ProcessContainer::new();
    let mode = if pressure {
        "produce-pressure"
    } else {
        "produce-pending"
    };
    let output = succeeded(session_child(mode, &container.0, &container.0, "base"));
    assert!(
        output.contains("producer pending=true sources_started=false dirty_documents=4"),
        "{output}"
    );
    let root = PathBuf::from(
        output
            .lines()
            .find_map(|line| line.strip_prefix("fixture-root="))
            .expect("the producer reports its actual project root"),
    )
    .canonicalize()
    .unwrap();
    let actual_container = container.0.canonicalize().unwrap();
    assert!(root.starts_with(&actual_container) && root != actual_container);
    let oracle = serde_json::from_slice(&fs::read(container.0.join(ORACLE)).unwrap()).unwrap();
    assert!(matches!(
        manifest(&root).authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    ProcessProject {
        root,
        oracle,
        _container: container,
    }
}

fn recovery(project: &ProcessProject, action: &str, stop: &[&str]) -> Value {
    let mut command = Command::new(companion());
    command
        .arg(action)
        .arg(&project.root)
        .args([
            "--host-work",
            "200000000",
            "--open-handles",
            "128",
            "--max-steps",
            "128",
        ])
        .args(stop)
        .env_remove(MODE_ENV)
        .env_remove(ROOT_ENV)
        .env_remove(PROFILE_ENV);
    let output = succeeded(run(command));
    serde_json::from_str(&output)
        .unwrap_or_else(|error| panic!("invalid companion JSON: {error}: {output}"))
}

fn ordinary_open(
    root: &Path,
    profile: &str,
) -> Result<EditorProjectSession, EditorProjectOpenFailure> {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(
        test_host_fs::writable_directory(root).unwrap(),
        (profile != "base").then(|| profile.to_owned()),
    );
    let deadline = Instant::now() + WAIT;
    let result = loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Opened(editor) => break Ok(editor),
            EditorProjectOpenProgress::Rejected(failure) => break Err(failure),
            EditorProjectOpenProgress::Pending(next) => attempt = next,
        }
        assert!(Instant::now() < deadline, "fresh ordinary Open must settle");
        thread::sleep(Duration::from_millis(1));
    };
    let deadline = Instant::now() + WAIT;
    loop {
        let report = host.poll_shutdown();
        if report.is_complete() {
            assert!(!report.has_cleanup_failure(), "{report:?}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the independent Open Host must physically close"
        );
        thread::sleep(Duration::from_millis(1));
    }
    result
}

fn assert_pending_open(project: &ProcessProject) {
    for profile in ["base", "dormant"] {
        let before = snapshot(&project.root);
        let output = succeeded(session_child(
            "open-pending",
            &project.root,
            &project._container.0,
            profile,
        ));
        assert!(
            output.contains(
                "ordinary-open pending=true code=project.editor.source-authoring-pending"
            ),
            "{output}"
        );
        assert_eq!(
            snapshot(&project.root),
            before,
            "ordinary Pending Open is read-only for {profile}"
        );
    }
}

fn published_outputs(project: &ProcessProject) -> Vec<(PathBuf, FileCapability)> {
    project
        .oracle
        .outputs
        .iter()
        .filter_map(|(path, expected)| {
            let actual = project.root.join(path);
            if actual.is_file() && fs::read(&actual).unwrap() == *expected {
                Some((path.clone(), test_host_fs::readable_file(&actual).unwrap()))
            } else {
                None
            }
        })
        .collect()
}

fn assert_identity_preserved(project: &ProcessProject, retained: &[(PathBuf, FileCapability)]) {
    for (path, file) in retained {
        assert!(
            file.same_object(&test_host_fs::readable_file(&project.root.join(path)).unwrap())
                .unwrap(),
            "already-new output was replaced: {}",
            path.display()
        );
    }
}

fn assert_inspection(project: &ProcessProject, report: &Value) {
    assert_eq!(report["status"], "inspected", "{report}");
    assert_eq!(report["phase"], "review", "{report}");
    assert_eq!(report["source_count"], project.oracle.source_count);
    assert_eq!(report["file_count"], project.oracle.outputs.len());
    assert_eq!(report["limits"]["host_work"], 200_000_000);
    assert_eq!(report["limits"]["open_handles"], 128);
    assert_eq!(report["ready_published"], false);
    assert!(report["refusal"].is_null(), "{report}");
    let review = &report["review"];
    assert!(review["selected_profile"].is_null());
    assert_eq!(review["selected_root"], "branch");
    assert_eq!(
        review["selected_source"],
        serde_json::to_value(key(P, SourceDocumentKind::Prefab)).unwrap()
    );
    assert_eq!(review["unchanged_bundle_observed"], "unchanged");
    let originals = review["originals"].as_array().unwrap();
    assert_eq!(originals.len(), project.oracle.source_count);
    let current = snapshot(&project.root);
    let mut paths = Vec::new();
    for original in originals {
        let path = Path::new(original["path"].as_str().unwrap());
        assert!(
            !paths.contains(&path),
            "duplicate original in the public Review"
        );
        paths.push(path);
        assert_eq!(
            original["original_disk_bytes"],
            project.oracle.original[path].len()
        );
        assert_eq!(
            original["dirty"],
            [STARTUP, P, INACTIVE, GUARD]
                .iter()
                .any(|value| path == Path::new(value))
        );
        let disposition = if path == Path::new(STARTUP) || path == Path::new(P) {
            "replace-source"
        } else if path == Path::new(INACTIVE) {
            "save-current"
        } else if path == Path::new(GUARD) {
            "guard"
        } else {
            "rewrite-closed"
        };
        assert_eq!(original["disposition"], disposition);
        match project.oracle.outputs.get(path) {
            Some(bytes) => {
                assert_eq!(original["output_bytes"], bytes.len());
                assert_eq!(
                    original["observed"],
                    if current[path] == *bytes {
                        "output"
                    } else {
                        "original"
                    }
                );
            }
            None => {
                assert!(original["output_bytes"].is_null());
                assert_eq!(original["observed"], "unchanged");
            }
        }
    }
    for (entry, path) in [(&review["q"], Q), (&review["q"]["metadata"], Q_META)] {
        assert_eq!(entry["path"], path);
        assert_eq!(
            entry["output_bytes"],
            project.oracle.outputs[Path::new(path)].len()
        );
        assert_eq!(
            entry["observed"],
            if current.contains_key(Path::new(path)) {
                "output"
            } else {
                "not-created"
            }
        );
    }
}

fn inspect_read_only(project: &ProcessProject) -> Value {
    let before = snapshot(&project.root);
    let report = recovery(project, "inspect", &[]);
    assert_inspection(project, &report);
    assert_eq!(
        recovery(project, "inspect", &[]),
        report,
        "inspection JSON is deterministic"
    );
    assert_eq!(
        snapshot(&project.root),
        before,
        "inspection leaves the complete tree unchanged"
    );
    report
}

fn assert_ready(project: &ProcessProject) {
    let actual = manifest(&project.root);
    assert_eq!(actual.authoring.state, ProjectAuthoringState::Ready);
    let original =
        ProjectManifest::parse_toml_bytes(&project.oracle.original[Path::new("nara.toml")]);
    assert_eq!(
        actual,
        original.manifest.unwrap(),
        "all original profile/settings declarations survive"
    );
    for (path, original) in &project.oracle.original {
        if path != Path::new("nara.toml") {
            let expected = project.oracle.outputs.get(path).unwrap_or(original);
            assert_eq!(
                &fs::read(project.root.join(path)).unwrap(),
                expected,
                "{}",
                path.display()
            );
        }
    }
    for (path, bytes) in &project.oracle.outputs {
        assert_eq!(
            &fs::read(project.root.join(path)).unwrap(),
            bytes,
            "{}",
            path.display()
        );
    }
    // The dormant declaration and its equal-spelling local entity survived the real extraction,
    // while the dirty Guard remained its old saved file and the schema bundle never changed.
    let dormant =
        SceneDocumentCandidate::decode_json_bytes(&fs::read(project.root.join(DORMANT)).unwrap())
            .unwrap()
            .into_composition_source()
            .unwrap();
    assert_routed_suppression(&dormant.entities, "left");
    assert!(
        dormant
            .entities
            .iter()
            .any(|entity| entity.id == id("child"))
    );
    assert_eq!(
        fs::read(project.root.join(GUARD)).unwrap(),
        project.oracle.original[Path::new(GUARD)]
    );
    assert_eq!(
        fs::read(project.root.join(BUNDLE)).unwrap(),
        project.oracle.original[Path::new(BUNDLE)]
    );
    let before = snapshot(&project.root);
    for profile in ["base", "dormant"] {
        let output = succeeded(session_child(
            "open-ready",
            &project.root,
            &project._container.0,
            profile,
        ));
        assert!(
            output.contains("ordinary-open ready=true history=0/0"),
            "{output}"
        );
        assert_eq!(
            snapshot(&project.root),
            before,
            "fresh Ready Open must not republish files"
        );
    }
}

fn limited_recovery(root: &Path) {
    let limits = AuthoringSourceOperationLimits::default();
    assert_eq!(limits.open_handles().get(), 64);
    let inspection = PendingAuthoringRecoveryInspection::inspect(
        test_host_fs::writable_directory(root).unwrap(),
        limits,
    )
    .unwrap();
    assert_eq!(
        inspection.kind(),
        PendingAuthoringRecoveryKind::PrefabExtraction
    );
    let mut recovery =
        PrefabExtractionRecovery::inspect(inspection, SceneCompilationLimits::default()).unwrap();
    for _ in 0..128 {
        if recovery.view().phase == RecoveryPhase::Review {
            break;
        }
        assert!(recovery.advance().is_ok(), "{:?}", recovery.failure());
    }
    assert_eq!(
        recovery.view().phase,
        RecoveryPhase::Review,
        "the default64 control is a late refusal"
    );
    assert_eq!(
        recovery.review().unwrap().sources().len(),
        SOURCES.len() + PRESSURE_SOURCES
    );
    let operation = recovery.view().operation;
    recovery.confirm().unwrap();
    for _ in 0..128 {
        if recovery.advance().is_err() {
            break;
        }
        assert_ne!(
            recovery.view().phase,
            RecoveryPhase::Complete,
            "the real pressure corpus must exceed default64"
        );
    }
    assert_eq!(
        recovery.failure().unwrap().kind(),
        RecoveryError::OpenHandleLimit
    );
    assert!(matches!(
        manifest(root).authoring.state,
        ProjectAuthoringState::Pending(_)
    ));
    let usage = recovery.view().usage;
    let disk = snapshot(root);
    assert_eq!(
        recovery.retry().unwrap_err(),
        RecoveryError::OpenHandleLimit
    );
    assert_eq!(
        recovery.view().usage,
        usage,
        "Retry cannot restart capacity accounting"
    );
    assert_eq!(recovery.view().operation, operation);
    assert_eq!(recovery.view().limits, limits);
    let closed = recovery.close().unwrap();
    assert_eq!(closed.phase, RecoveryPhase::Closed);
    assert_eq!(closed.limits, limits);
    assert_eq!(
        closed.usage, usage,
        "Close cannot refund cumulative work or historical peak"
    );
    assert_eq!(
        snapshot(root),
        disk,
        "Retry and Close cannot restart source publication"
    );
    let retained = recovery.into_outcome().unwrap_err();
    assert_eq!(retained.view().operation, operation);
    assert_eq!(retained.view().phase, RecoveryPhase::Closed);
    println!(
        "\nlimited-recovery refusal=OpenHandleLimit selected=64 retry=OpenHandleLimit close=closed allowance_preserved=true"
    );
}

#[test]
#[ignore = "subprocess entry; the subprocess qualification tests supply its environment"]
fn child_entry() {
    let Some(mode) = std::env::var_os(MODE_ENV) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("the child root is explicit"));
    let profile = std::env::var(PROFILE_ENV).expect("the child profile is explicit");
    match mode.to_str().unwrap() {
        "produce-pending" => produce_pending(&root, false),
        "produce-pressure" => produce_pending(&root, true),
        "open-pending" => {
            let before = snapshot(&root);
            let failure = ordinary_open(&root, &profile)
                .err()
                .expect("ordinary Open refuses Pending");
            assert_eq!(failure.phase(), EditorProjectOpenPhase::ManifestAndProfile);
            assert_eq!(
                failure.source(),
                EditorProjectOpenFailureSource::ProjectManifest
            );
            assert!(
                failure
                    .diagnostics()
                    .iter()
                    .any(|diagnostic| diagnostic.code().as_str()
                        == "project.editor.source-authoring-pending"),
                "{:?}",
                failure.diagnostics()
            );
            assert_eq!(snapshot(&root), before);
            println!("\nordinary-open pending=true code=project.editor.source-authoring-pending");
        }
        "open-ready" => {
            assert_eq!(
                manifest(&root).authoring.state,
                ProjectAuthoringState::Ready
            );
            let mut editor = ordinary_open(&root, &profile).unwrap();
            let document = editor.workspace().active_document().unwrap();
            let source = state(&editor, document);
            assert!(!source.dirty);
            assert_eq!(source.history.undo_depth, 0);
            assert_eq!(source.history.redo_depth, 0);
            assert!(source.encoded.contains(if profile == "base" {
                "dirty-startup"
            } else {
                "\"left\""
            }));
            close(&mut editor);
            // A fresh authoring Open is not strict native Check/Play evidence. Those consumers
            // are qualified separately with a product recipe and actual executable bindings.
            println!("\nordinary-open ready=true history=0/0 check=unqualified play=unqualified");
        }
        "limited-recovery" => limited_recovery(&root),
        other => panic!("unknown subprocess mode: {other}"),
    }
}

#[test]
#[ignore = "requires NARA_PREFAB_EXTRACTION_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_interruption_matrix_recovers_exact_review_and_preserves_already_new_identity() {
    for (boundary, stop, completed) in [
        ("all-old", &(["--stop-after-files", "0"])[..], 0),
        ("q-only", &(["--stop-after-files", "1"])[..], 1),
        ("q-and-metadata", &(["--stop-after-files", "2"])[..], 2),
        ("partial-original", &(["--stop-after-files", "3"])[..], 3),
        ("all-output-pending", &(["--stop-after-files", "8"])[..], 8),
        ("before-ready", &(["--stop-before-ready"])[..], 8),
    ] {
        let project = pending_project(false);
        let initial = inspect_read_only(&project);
        assert_pending_open(&project);
        let stopped = recovery(&project, "resume", stop);
        assert_eq!(stopped["status"], "stopped", "{boundary}: {stopped}");
        assert_eq!(
            stopped["completed_files"], completed,
            "{boundary}: {stopped}"
        );
        assert_eq!(stopped["ready_published"], false, "{boundary}: {stopped}");
        if boundary == "before-ready" {
            assert_eq!(stopped["phase"], "publishing-ready");
        }
        assert!(matches!(
            manifest(&project.root).authoring.state,
            ProjectAuthoringState::Pending(_)
        ));
        let retained = published_outputs(&project);
        assert_eq!(
            retained.len(),
            completed,
            "{boundary}: actual fixed outputs, not just a counter"
        );
        assert_eq!(project.root.join(Q).exists(), completed >= 1);
        assert_eq!(project.root.join(Q_META).exists(), completed >= 2);
        let mixed = inspect_read_only(&project);
        assert_eq!(initial["operation"], mixed["operation"]);
        assert_eq!(initial["review"]["schema"], mixed["review"]["schema"]);
        assert_pending_open(&project);
        let complete = recovery(&project, "resume", &["--all"]);
        assert_eq!(complete["status"], "complete", "{boundary}: {complete}");
        assert_eq!(complete["phase"], "complete");
        assert_eq!(complete["outcome_consumed"], true);
        assert_eq!(complete["outcome"]["schema_basis_preserved"], true);
        assert_eq!(
            complete["outcome"]["ready_receipt"]["published_is_candidate"],
            true
        );
        assert_eq!(complete["operation"], initial["operation"]);
        assert_identity_preserved(&project, &retained);
        assert_ready(&project);
    }
}

#[test]
#[ignore = "requires NARA_PREFAB_EXTRACTION_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_ready_before_outcome_acknowledgment_opens_in_both_real_profiles() {
    let project = pending_project(false);
    let report = recovery(&project, "resume", &["--stop-after-ready"]);
    assert_eq!(report["status"], "stopped", "{report}");
    assert_eq!(report["phase"], "complete");
    assert_eq!(report["ready_published"], true);
    assert_eq!(
        report["outcome_consumed"], false,
        "the producing process did not acknowledge an outcome"
    );
    let retained = published_outputs(&project);
    assert_eq!(retained.len(), project.oracle.outputs.len());
    assert_ready(&project);
    assert_identity_preserved(&project, &retained);
}

#[test]
#[ignore = "requires NARA_PREFAB_EXTRACTION_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_late_default64_refusal_keeps_its_allowance_before_fresh_selected128_recovery() {
    let project = pending_project(true);
    let initial = inspect_read_only(&project);
    let mut command = Command::new(companion());
    command
        .arg("resume")
        .arg(&project.root)
        .args([
            "--host-work",
            "200000000",
            "--open-handles",
            "64",
            "--max-steps",
            "128",
            "--all",
        ])
        .env_remove(MODE_ENV)
        .env_remove(ROOT_ENV)
        .env_remove(PROFILE_ENV);
    let refused = run(command);
    assert_eq!(refused.status.code(), Some(2), "{refused:?}");
    let refused: Value = serde_json::from_slice(&refused.stdout).unwrap();
    assert_eq!(refused["refusal"], "OpenHandleLimit", "{refused}");
    assert_eq!(refused["limits"]["open_handles"], 64);
    assert_eq!(refused["operation"], initial["operation"]);
    assert_eq!(refused["ready_published"], false);
    assert_eq!(refused["close"]["resolved"], true);
    assert_eq!(refused["close"]["phase"], "closed");
    let cli_outputs = published_outputs(&project);
    assert!(
        !cli_outputs.is_empty(),
        "the CLI-only capacity control must fail after physical progress"
    );
    let output = succeeded(session_child(
        "limited-recovery",
        &project.root,
        &project._container.0,
        "base",
    ));
    assert!(output.contains("limited-recovery refusal=OpenHandleLimit selected=64 retry=OpenHandleLimit close=closed allowance_preserved=true"), "{output}");
    assert_identity_preserved(&project, &cli_outputs);
    assert_pending_open(&project);
    let retained = published_outputs(&project);
    assert!(
        !retained.is_empty(),
        "the native-capacity refusal occurs after actual source progress"
    );
    inspect_read_only(&project);
    let complete = recovery(&project, "resume", &["--all"]);
    assert_eq!(complete["status"], "complete", "{complete}");
    assert_eq!(complete["limits"]["open_handles"], 128);
    assert_identity_preserved(&project, &retained);
    assert_ready(&project);
}
