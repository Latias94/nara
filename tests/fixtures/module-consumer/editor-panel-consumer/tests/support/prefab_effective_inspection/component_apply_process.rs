use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use nara::{
    ecs::Resource,
    fs::RelativePath,
    project_host::{
        AuthoringSourceOperationLimits, HeadlessRun, HeadlessRunOutcome,
        PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryKind,
        PrefabComponentApplyRecovery, PrefabComponentApplyRecoveryErrorKind,
        PrefabComponentApplyRecoveryPhase,
    },
    project_workflow::{ProjectCheckRequest, check_project},
};
use serde_json::{Value, json};

use super::{
    Application, BASE, EditorApplication, EditorLaunchOptions, SCENE,
    component_apply::at_first_added_component_apply_output, open_source, wait,
};
use crate::text_project::{TextProject, recipe};

const MODE: &str = "NARA_COMPONENT_APPLY_PROCESS_MODE";
const ORACLE: &str = "NARA_COMPONENT_APPLY_PROCESS_ORACLE";
const FIRST_OUTPUT_EXIT: i32 = 17;
const PENDING: &str = "runtime.configuration.source-authoring-pending";

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let (Ok(path), Ok(temp)) = (self.0.canonicalize(), std::env::temp_dir().canonicalize())
        else {
            return;
        };
        if path.parent() == Some(temp.as_path())
            && path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("nara-component-apply-process-")
                    || name.starts_with("nara-editor-text-")
            })
        {
            let _ = fs::remove_dir_all(path);
        }
    }
}

fn run_child(scratch: &Path, mode: &str) {
    let stdout = scratch.join(format!("{mode}.out"));
    let stderr = scratch.join(format!("{mode}.err"));
    let test = concat!(module_path!(), "::child_entry")
        .split_once("::")
        .unwrap()
        .1;
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env(MODE, mode)
        .env(ORACLE, scratch.join("review.json"))
        .stdout(Stdio::from(fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr).unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("the {mode} child did not return");
        }
        thread::sleep(Duration::from_millis(5));
    };
    let expected = if mode == "produce" {
        Some(FIRST_OUTPUT_EXIT)
    } else {
        Some(0)
    };
    assert_eq!(
        status.code(),
        expected,
        "component Apply {mode} child: {}\n{}",
        fs::read_to_string(stdout).unwrap(),
        fs::read_to_string(stderr).unwrap(),
    );
}

#[test]
fn child_entry() {
    let Some(mode) = std::env::var_os(MODE) else {
        return;
    };
    let oracle = PathBuf::from(std::env::var_os(ORACLE).unwrap());
    if mode == "produce" {
        at_first_added_component_apply_output(|root, originals, outputs| {
            let receipt = json!({
                "root": root,
                "originals": originals,
                "outputs": outputs,
            });
            fs::write(oracle, serde_json::to_vec(&receipt).unwrap()).unwrap();
            std::process::exit(FIRST_OUTPUT_EXIT);
        });
        unreachable!("the producer exits with one output accepted");
    }
    assert_eq!(mode, "recover");
    let receipt: Value = serde_json::from_slice(&fs::read(oracle).unwrap()).unwrap();
    let project = std::mem::ManuallyDrop::new(TextProject {
        root: serde_json::from_value(receipt["root"].clone()).unwrap(),
    });
    let inspection = PendingAuthoringRecoveryInspection::inspect(
        project.capability(),
        AuthoringSourceOperationLimits::default(),
    )
    .unwrap();
    assert_eq!(
        inspection.kind(),
        PendingAuthoringRecoveryKind::PrefabComponentApply
    );
    let mut recovery = PrefabComponentApplyRecovery::from_inspection(inspection).unwrap();
    let [first, second] = recovery.review().unwrap().outputs();
    let outputs: [Vec<u8>; 2] = serde_json::from_value(receipt["outputs"].clone()).unwrap();
    assert_eq!([first.project_path, second.project_path], [SCENE, BASE]);
    assert!(first.already_current);
    assert!(!second.already_current);
    assert_eq!(
        [first.output, second.output],
        outputs.each_ref().map(Vec::as_slice)
    );
    let progress = recovery.confirm().unwrap();
    assert_eq!(
        progress.phase,
        PrefabComponentApplyRecoveryPhase::PublishingSources
    );
    for expected in 1..=2 {
        assert_eq!(recovery.advance().unwrap().completed_files, expected);
    }
    assert_eq!(
        recovery.advance().unwrap().phase,
        PrefabComponentApplyRecoveryPhase::Complete
    );
    let receipt = recovery.into_receipt().unwrap();
    assert_eq!(receipt.written_sources(), 1);
    assert!(
        receipt
            .ready_publication()
            .published_content_digest()
            .is_some()
    );
    let mut app = Application(EditorApplication::new(EditorLaunchOptions::new(
        &project.root,
    )));
    app.native_shell_ready();
    wait(&mut app, |current| {
        !current.project_browser().has_pending_work()
    });
    assert_ne!(open_source(&mut app, SCENE), open_source(&mut app, BASE));
    assert!(app.close());
}

#[derive(Clone, Resource)]
struct Probe;

#[test]
fn interrupted_component_apply_blocks_startup_and_recovers_only_the_missing_output() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "nara-component-apply-process-{}",
        std::process::id(),
    )));
    fs::create_dir(&scratch.0).unwrap();
    run_child(&scratch.0, "produce");
    let receipt: Value =
        serde_json::from_slice(&fs::read(scratch.0.join("review.json")).unwrap()).unwrap();
    let root: PathBuf = serde_json::from_value(receipt["root"].clone()).unwrap();
    let project_cleanup = Scratch(root.clone());
    let project = std::mem::ManuallyDrop::new(TextProject { root });
    let originals: [Vec<u8>; 2] = serde_json::from_value(receipt["originals"].clone()).unwrap();
    let outputs: [Vec<u8>; 2] = serde_json::from_value(receipt["outputs"].clone()).unwrap();
    let root = &project_cleanup.0;
    let manifest = root.join("nara.toml");
    let pending_bytes = fs::read(&manifest).unwrap();
    assert_eq!(fs::read(root.join(SCENE)).unwrap(), outputs[0]);
    assert_eq!(fs::read(root.join(BASE)).unwrap(), originals[1]);
    let capability = project.capability();
    let relative = RelativePath::new(SCENE).unwrap();
    let first = capability.open_file(&relative).unwrap();
    let modified = fs::metadata(root.join(SCENE)).unwrap().modified().unwrap();

    let check = check_project(ProjectCheckRequest::new(capability.share(), recipe())).unwrap_err();
    assert!(
        check
            .manifest_error()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == PENDING)
    );
    let mut run = HeadlessRun::<Probe>::from_recipe(
        capability.share(),
        recipe(),
        NonZeroU32::MIN,
        Vec::new(),
    );
    let play = run.execute_bounded();
    assert!(matches!(play.outcome(), HeadlessRunOutcome::Failed));
    assert!(
        play.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code().as_str() == PENDING)
    );
    assert!(play.startup_content_closure().is_none());

    let reject_drift = |path: &str, original: &[u8]| {
        let path = root.join(path);
        let mut third = original.to_vec();
        third.push(b'\n');
        fs::write(&path, &third).unwrap();
        let error = PrefabComponentApplyRecovery::inspect(
            capability.share(),
            AuthoringSourceOperationLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            error.error().kind(),
            PrefabComponentApplyRecoveryErrorKind::Inventory
        );
        assert_eq!(fs::read(&path).unwrap(), third);
        assert_eq!(fs::read(&manifest).unwrap(), pending_bytes);
        fs::write(path, original).unwrap();
    };
    reject_drift(BASE, &originals[1]);
    let startup = fs::read(root.join("scenes/startup.scene.json")).unwrap();
    reject_drift("scenes/startup.scene.json", &startup);

    run_child(&scratch.0, "recover");
    let current = capability.open_file(&relative).unwrap();
    assert!(first.same_object(&current).unwrap());
    assert_eq!(
        fs::metadata(root.join(SCENE)).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(fs::read(root.join(SCENE)).unwrap(), outputs[0]);
    assert_eq!(fs::read(root.join(BASE)).unwrap(), outputs[1]);
    assert!(matches!(
        nara::project::ProjectManifest::parse_toml_bytes(&fs::read(&manifest).unwrap())
            .manifest
            .unwrap()
            .authoring
            .state,
        nara::project::ProjectAuthoringState::Ready,
    ));
    check_project(ProjectCheckRequest::new(capability.share(), recipe())).unwrap();
}
