//! Process interruption proof. Build the CLI companion separately; tests never invoke Cargo.
//!
//! Run the ignored `subprocess_` tests with NARA_AUTHORED_UPGRADE_RECOVERY_EXE pointing to
//! the companion. The producer and ordinary Open each run in a fresh copy of this test binary.

use std::{
    collections::BTreeMap,
    ffi::OsString,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use nara_engine::{
    core::ItemLimit,
    fs::{CapabilityRights, DirectoryCapability, FileCapability, HostCapabilityOptions, TrustMode},
    project::{ProjectAuthoringState, ProjectManifest},
    project_host::{
        AuthoringSourceOperationLimits, EditorProjectOpenFailure, EditorProjectOpenHost,
        EditorProjectOpenProgress,
    },
};

use super::*;

const COMPANION_ENV: &str = "NARA_AUTHORED_UPGRADE_RECOVERY_EXE";
const MODE_ENV: &str = "NARA_AUTHORED_UPGRADE_CHILD_MODE";
const ROOT_ENV: &str = "NARA_AUTHORED_UPGRADE_CHILD_ROOT";
const PROFILE_ENV: &str = "NARA_AUTHORED_UPGRADE_CHILD_PROFILE";
const CHILD_ENTRY: &str = "authored_upgrade_session::process_recovery::child_entry";

struct ProcessProject {
    root: PathBuf,
    _container: tempfile::TempDir,
}

fn companion() -> OsString {
    let executable = std::env::var_os(COMPANION_ENV).unwrap_or_else(|| {
        panic!("set {COMPANION_ENV} to the separately built CLI companion before qualification")
    });
    assert!(
        Path::new(&executable).is_file(),
        "{COMPANION_ENV} must name an existing executable",
    );
    executable
}

fn session_child(mode: &str, root: &Path, profile: &str) -> Output {
    Command::new(std::env::current_exe().unwrap())
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
        // Keep the producer's existing TempDir fixture inside the parent's cleanup owner.
        .env("TMP", root)
        .env("TEMP", root)
        .env("TMPDIR", root)
        .output()
        .expect("the public Session child must start")
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

fn pending_project(product_ready: bool) -> ProcessProject {
    // Fail before starting the producer if the externally built artifact is unavailable.
    let _ = companion();
    let container = tempfile::tempdir().unwrap();
    let mode = if product_ready {
        "produce-product-pending"
    } else {
        "produce-pending"
    };
    let output = succeeded(session_child(mode, container.path(), "base"));
    assert!(output.contains("producer pending=true sources_started=false dirty_health=109"));
    let root = PathBuf::from(
        output
            .lines()
            .find_map(|line| line.strip_prefix("fixture-root="))
            .expect("the producer must report its actual on-disk project root"),
    );
    let root = fs::canonicalize(root).unwrap();
    let container_path = fs::canonicalize(container.path()).unwrap();
    assert!(root.starts_with(&container_path) && root != container_path);
    assert!(matches!(
        manifest(&root).authoring.state,
        ProjectAuthoringState::Pending(_),
    ));
    ProcessProject {
        root,
        _container: container,
    }
}

fn recovery(root: &Path, action: &str, stop: &[&str]) -> Output {
    Command::new(companion())
        .arg(action)
        .arg(root)
        .args([
            "--profile",
            "base",
            "--host-work",
            "200000000",
            "--open-handles",
            "128",
            "--max-steps",
            "64",
        ])
        .args(stop)
        .env_remove(MODE_ENV)
        .env_remove(ROOT_ENV)
        .env_remove(PROFILE_ENV)
        .output()
        .expect("the independent recovery CLI must start")
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
                // Observe only after the actual publisher's exclusive range lock returns.
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("the project manifest observation failed: {error}"),
        }
    };
    let loaded = ProjectManifest::parse_toml_bytes(&bytes);
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    loaded.manifest.unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
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
                    "the process fixture contains only regular files",
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

fn assert_review_counts(output: &str, original: usize, successor: usize, bundle: &str) {
    let expected = format!(
        "review sources=6 original={original} successor={successor} unchanged=0 dirty=3 bundle={bundle}",
    );
    assert!(output.lines().any(|line| line == expected), "{output}");
}

fn assert_ready(output: &str, root: &Path) {
    assert!(
        output.contains("view phase=complete sources=6 completed=6 bundle=true"),
        "{output}",
    );
    assert!(
        output.contains("ready confirmed=true sources=6 content_verified=true"),
        "{output}",
    );
    assert_eq!(manifest(root).authoring.state, ProjectAuthoringState::Ready);
    assert_successor_sources(root);
}

fn assert_successor_sources(root: &Path) {
    // These complete trees contain literal milli values. No project converter computes an oracle.
    // Equality includes dormant pins, equal-to-source pins, additions, topology and opaque data.
    for path in SOURCES {
        let bytes = fs::read(root.join(path)).unwrap();
        match path {
            STARTUP | INACTIVE | PROFILE_SCENE => {
                let actual = SceneDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let expected = if path == PROFILE_SCENE {
                    profile_scene(true)
                } else {
                    scene_document(true, true)
                };
                assert_eq!(actual, expected, "{path}");
            }
            PREFAB | LEAF | PROFILE_PREFAB => {
                let actual = PrefabDocumentCandidate::decode_json_bytes(&bytes)
                    .unwrap()
                    .into_composition_source()
                    .unwrap();
                let expected = match path {
                    PREFAB => enemy_document(true, true),
                    LEAF => leaf_document(true),
                    PROFILE_PREFAB => profile_prefab(true),
                    _ => unreachable!(),
                };
                assert_eq!(actual, expected, "{path}");
            }
            _ => unreachable!(),
        }
    }
    let bundle = AuthoringSchemaBundle::from_json_str(
        &fs::read_to_string(root.join("schema/authoring.bundle.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        bundle
            .owner(&owner(OWNER))
            .unwrap()
            .schema(&component())
            .unwrap()
            .version(),
        V2,
    );
}

fn root_capability(root: &Path) -> DirectoryCapability {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000);
    }
    DirectoryCapability::from_host_handle(
        options.open(root).unwrap(),
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}

fn member_capability(path: &Path) -> FileCapability {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(7).custom_flags(0x0020_0000);
    }
    FileCapability::from_host_handle(options.open(path).unwrap(), TrustMode::TrustedLocal, 1)
        .unwrap()
}

fn ordinary_open(
    root: &Path,
    profile: &str,
) -> Result<EditorProjectSession, EditorProjectOpenFailure> {
    let mut host = EditorProjectOpenHost::new();
    let mut attempt = host.begin(
        root_capability(root),
        (profile != "base").then(|| profile.to_owned()),
    );
    let deadline = Instant::now() + WAIT;
    let result = loop {
        match host.advance(attempt) {
            EditorProjectOpenProgress::Opened(editor) => break Ok(editor),
            EditorProjectOpenProgress::Rejected(failure) => break Err(failure),
            EditorProjectOpenProgress::Pending(next) => attempt = next,
        }
        assert!(Instant::now() < deadline, "ordinary child Open must settle");
        std::thread::sleep(Duration::from_millis(1));
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
            "ordinary child Open Host must close"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    result
}

fn assert_pending_open(root: &Path) {
    let before = snapshot(root);
    let output = succeeded(session_child("open-pending", root, "base"));
    assert!(output.contains("ordinary-open pending=true"), "{output}");
    assert_eq!(
        snapshot(root),
        before,
        "ordinary Open must leave the complete Pending tree unchanged",
    );
}

fn produce_pending(container: &Path, product_ready: bool) -> ! {
    let project = if product_ready {
        product::product_project(false)
    } else {
        fixture_project()
    };
    let root = fs::canonicalize(project.root()).unwrap();
    let container = fs::canonicalize(container).unwrap();
    assert!(root.starts_with(&container) && root != container);
    let disk = TRACKED.map(|path| fs::read(project.root().join(path)).unwrap());
    let (mut editor, documents) = open_dirty(&project);
    let operation = editor
        .begin_authored_schema_upgrade_with_limits(
            AuthoringSourceOperationLimits::default()
                .with_host_work(ItemLimit::new(200_000_000).unwrap())
                .with_open_handles(ItemLimit::new(128).unwrap()),
        )
        .unwrap();
    wait(&mut editor, |editor| {
        !matches!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::Draining
                | EditorAuthoredSchemaUpgradePhase::Capturing
        )
    });
    assert_eq!(
        editor.authored_schema_upgrade_view().phase,
        EditorAuthoredSchemaUpgradePhase::AwaitingProposal,
    );
    let proposal = proposed(&editor, operation);
    editor
        .prepare_authored_schema_upgrade(operation, proposal)
        .unwrap();
    settle_preparation(&mut editor);
    if product_ready {
        let review = editor.authored_schema_upgrade_review(operation).unwrap();
        assert_eq!(review.source_count(), 7);
        assert_eq!(review.document_count(), documents.len());
    } else {
        assert_review(&editor, operation, documents, &disk);
    }
    editor.confirm_authored_schema_upgrade(operation).unwrap();
    wait(&mut editor, |editor| {
        matches!(
            editor.authored_schema_upgrade_view().phase,
            EditorAuthoredSchemaUpgradePhase::PublishingPending
                | EditorAuthoredSchemaUpgradePhase::RequiresReconcile
                | EditorAuthoredSchemaUpgradePhase::Terminal
        )
    });
    assert_eq!(
        editor.authored_schema_upgrade_view().phase,
        EditorAuthoredSchemaUpgradePhase::PublishingPending,
    );
    // This worker can publish only Pending. Do not drive another frame: returning its result
    // would enqueue the first source step. Process exit intentionally skips Session/TempDir Drop.
    let deadline = Instant::now() + WAIT;
    while !matches!(
        manifest(project.root()).authoring.state,
        ProjectAuthoringState::Pending(_),
    ) {
        assert!(
            Instant::now() < deadline,
            "the actual Pending replacement must become visible",
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    for (path, bytes) in TRACKED.into_iter().zip(disk).skip(1) {
        assert_eq!(
            fs::read(project.root().join(path)).unwrap(),
            bytes,
            "{path}"
        );
    }
    if product_ready {
        let extra = fs::read(project.root().join("dormant/scenes/inactive.scene.json")).unwrap();
        assert_eq!(
            extra,
            profile_scene(false).to_json_string().unwrap().into_bytes()
        );
    }
    println!("\nfixture-root={}", project.root().display());
    println!("producer pending=true sources_started=false dirty_health=109");
    std::io::stdout().flush().unwrap();
    std::process::exit(0);
}

#[test]
#[ignore = "subprocess entry; the subprocess qualification tests supply its environment"]
fn child_entry() {
    let Some(mode) = std::env::var_os(MODE_ENV) else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("child root is explicit"));
    let profile = std::env::var(PROFILE_ENV).expect("child profile is explicit");
    match mode.to_str().unwrap() {
        "produce-pending" => {
            assert_eq!(profile, "base");
            produce_pending(&root, false);
        }
        "produce-product-pending" => {
            assert_eq!(profile, "base");
            produce_pending(&root, true);
        }
        "product-ready" => {
            assert_eq!(
                manifest(&root).authoring.state,
                ProjectAuthoringState::Ready
            );
            product::assert_saved_products(&root);
            let mut editor = ordinary_open(&root, "base").unwrap();
            let document = editor.current_frame().workspace().active_document.unwrap();
            project::assert_history(&editor, document, 0, 0);
            product::ordinary_followup(&mut editor, document);
            project::close(&mut editor);
            drop(editor);
            product::assert_saved_editor_play(&root);
            println!("\nrecovered-product v2=true check=true runtime=true play=true history=fresh");
        }
        "open-pending" => {
            let failure = ordinary_open(&root, &profile)
                .err()
                .expect("ordinary Open must refuse Pending");
            assert!(
                failure.diagnostics().iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "project.editor.source-authoring-pending"
                }),
                "{:?}",
                failure.diagnostics(),
            );
            println!("\nordinary-open pending=true");
        }
        "open-ready" => {
            assert_eq!(
                manifest(&root).authoring.state,
                ProjectAuthoringState::Ready
            );
            assert_successor_sources(&root);
            let mut editor = ordinary_open(&root, &profile).unwrap();
            let startup = editor.current_frame().workspace().active_document.unwrap();
            project::assert_history(&editor, startup, 0, 0);
            let (entity, milli) = if profile == "base" {
                ("enemy", 109000)
            } else {
                ("profile-enemy", 19000)
            };
            project::assert_value(
                &editor,
                startup,
                entity,
                COMPONENT,
                "health",
                ComponentValue::map([("milli", ComponentValue::I64(milli))]),
            );
            project::close(&mut editor);
            println!("\nordinary-open ready=true history=0/0 milli={milli}");
        }
        other => panic!("unknown subprocess mode: {other}"),
    }
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_pending_all_old_recovers_reviewed_dirty_sources_and_dormant_overlays() {
    let project = pending_project(false);
    let before = snapshot(&project.root);
    let output = succeeded(recovery(&project.root, "inspect", &[]));
    assert_review_counts(&output, 6, 0, "original");
    assert!(output.contains("view phase=review sources=6 completed=0 bundle=false"));
    assert_eq!(snapshot(&project.root), before, "inspection is read-only");
    assert_pending_open(&project.root);
    let output = succeeded(recovery(&project.root, "resume", &["--all"]));
    assert_review_counts(&output, 6, 0, "original");
    assert_ready(&output, &project.root);
    let output = succeeded(session_child("open-ready", &project.root, "base"));
    assert!(output.contains("ordinary-open ready=true history=0/0 milli=109000"));
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_mixed_sources_and_successor_bundle_preserve_already_new_file_identity() {
    let project = pending_project(false);
    let original = SOURCES.map(|path| fs::read(project.root.join(path)).unwrap());
    let output = succeeded(recovery(
        &project.root,
        "resume",
        &["--stop-after-sources", "2"],
    ));
    assert!(output.contains("view phase=publishing-sources sources=6 completed=2 bundle=false"));
    let output = succeeded(recovery(&project.root, "inspect", &[]));
    assert_review_counts(&output, 4, 2, "original");
    assert_pending_open(&project.root);
    let new_index = SOURCES
        .iter()
        .enumerate()
        .find_map(|(index, path)| {
            (fs::read(project.root.join(path)).unwrap() != original[index]).then_some(index)
        })
        .unwrap();
    let retained_path = project.root.join(SOURCES[new_index]);
    let retained = member_capability(&retained_path);
    let output = succeeded(recovery(
        &project.root,
        "resume",
        &["--stop-after-sources", "all"],
    ));
    assert!(output.contains("sources=6 completed=6 bundle=true"));
    assert!(
        retained
            .same_object(&member_capability(&retained_path))
            .unwrap()
    );
    // Also qualify the supported new-bundle/old-member observation. This test deliberately
    // reinstates one exact old member after the bounded child published the bundle.
    let old_index = (0..SOURCES.len())
        .find(|index| *index != new_index)
        .unwrap();
    fs::write(project.root.join(SOURCES[old_index]), &original[old_index]).unwrap();
    let output = succeeded(recovery(&project.root, "inspect", &[]));
    assert_review_counts(&output, 1, 5, "successor");
    assert_pending_open(&project.root);
    let output = succeeded(recovery(&project.root, "resume", &["--all"]));
    assert_review_counts(&output, 1, 5, "successor");
    assert_ready(&output, &project.root);
    assert!(
        retained
            .same_object(&member_capability(&retained_path))
            .unwrap(),
        "observing new bytes must not rewrite the already-new member",
    );
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_all_new_pending_publishes_only_ready_without_multiplying_again() {
    let project = pending_project(false);
    let output = succeeded(recovery(
        &project.root,
        "resume",
        &["--stop-after-sources", "all"],
    ));
    assert!(output.contains("view phase=preparing-ready sources=6 completed=6 bundle=true"));
    assert_successor_sources(&project.root);
    assert_pending_open(&project.root);
    let before = SOURCES.map(|path| fs::read(project.root.join(path)).unwrap());
    let retained = SOURCES.map(|path| member_capability(&project.root.join(path)));
    let bundle = member_capability(&project.root.join("schema/authoring.bundle.json"));
    let output = succeeded(recovery(&project.root, "resume", &["--all"]));
    assert_review_counts(&output, 0, 6, "successor");
    assert_ready(&output, &project.root);
    assert_eq!(
        SOURCES.map(|path| fs::read(project.root.join(path)).unwrap()),
        before,
    );
    for (path, retained) in SOURCES.into_iter().zip(retained) {
        assert!(
            retained
                .same_object(&member_capability(&project.root.join(path)))
                .unwrap(),
            "already-new member was replaced: {path}",
        );
    }
    assert!(
        bundle
            .same_object(&member_capability(
                &project.root.join("schema/authoring.bundle.json")
            ))
            .unwrap(),
    );
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_external_source_metadata_and_recovery_digest_conflicts_keep_pending() {
    for conflict in ["source", "metadata", "recovery-digest"] {
        let project = pending_project(false);
        match conflict {
            "source" => {
                // The reviewed dirty v1 value is neither the old disk value nor its v2 output.
                fs::write(
                    project.root.join(STARTUP),
                    scene_document(false, true).to_json_string().unwrap(),
                )
                .unwrap();
            }
            "metadata" => {
                let path = project.root.join("assets/textures/sensor.png.meta");
                let metadata = fs::read_to_string(&path).unwrap();
                let changed = metadata.replace(
                    "b1a29038-b6ee-44b9-aa80-15be0560ed4d",
                    "b1a29038-b6ee-44b9-aa80-15be0560ed4e",
                );
                assert_ne!(
                    metadata, changed,
                    "the competing sidecar must change identity",
                );
                fs::write(path, changed).unwrap();
            }
            "recovery-digest" => {
                let ProjectAuthoringState::Pending(reference) =
                    manifest(&project.root).authoring.state
                else {
                    panic!("producer must leave Pending");
                };
                let path = project.root.join(reference.recovery_path().as_str());
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b' ');
                fs::write(path, bytes).unwrap();
            }
            _ => unreachable!(),
        }
        let before = snapshot(&project.root);
        for (action, stop) in [("inspect", &[][..]), ("resume", &["--all"][..])] {
            let output = recovery(&project.root, action, stop);
            assert_eq!(output.status.code(), Some(2), "{conflict}: {output:?}");
            let error = String::from_utf8(output.stderr).unwrap();
            let kind = if conflict == "recovery-digest" {
                "Pending"
            } else {
                "Inventory"
            };
            assert!(
                error.contains(&format!("inspect refused kind={kind}:")),
                "{conflict}: {error}",
            );
            assert_eq!(
                snapshot(&project.root),
                before,
                "{conflict}: refusal must not change any fixture file",
            );
        }
        assert!(matches!(
            manifest(&project.root).authoring.state,
            ProjectAuthoringState::Pending(_),
        ));
        assert_pending_open(&project.root);
    }
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_ready_before_ack_opens_independently_in_base_and_dormant_profiles() {
    let project = pending_project(false);
    let output = succeeded(recovery(&project.root, "resume", &["--stop-after-ready"]));
    assert!(output.contains("view phase=complete sources=6 completed=6 bundle=true"));
    assert!(output.contains("ready receipt=unconsumed acknowledged=false"));
    assert_eq!(
        manifest(&project.root).authoring.state,
        ProjectAuthoringState::Ready
    );
    assert_successor_sources(&project.root);
    let before = snapshot(&project.root);
    for (profile, expected) in [("base", 109000), ("dormant", 19000)] {
        let output = succeeded(session_child("open-ready", &project.root, profile));
        assert!(
            output.contains(&format!(
                "ordinary-open ready=true history=0/0 milli={expected}"
            )),
            "{output}",
        );
        assert_eq!(
            snapshot(&project.root),
            before,
            "fresh ordinary Open must not repeat conversion or save",
        );
    }
}

#[test]
#[ignore = "requires NARA_AUTHORED_UPGRADE_RECOVERY_EXE built separately with project-workflow-cli only"]
fn subprocess_recovered_product_checks_executes_and_plays_v2_with_fresh_history() {
    let project = pending_project(true);
    assert_pending_open(&project.root);
    let inspected = succeeded(recovery(&project.root, "inspect", &[]));
    assert!(
        inspected.contains("review sources=7 original=7 successor=0"),
        "{inspected}"
    );
    let recovered = succeeded(recovery(&project.root, "resume", &["--all"]));
    assert!(
        recovered.contains("ready confirmed=true sources=7 content_verified=true"),
        "{recovered}"
    );
    let output = succeeded(session_child("product-ready", &project.root, "base"));
    assert!(
        output
            .contains("recovered-product v2=true check=true runtime=true play=true history=fresh"),
        "{output}"
    );
    assert_eq!(
        manifest(&project.root).authoring.state,
        ProjectAuthoringState::Ready
    );
}
