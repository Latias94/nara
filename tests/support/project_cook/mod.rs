use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    fs::{ContentDigest, RelativePath},
    project_tooling::{
        cargo_build::{CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        host_fs,
        process::SupervisedProcessCommand,
        project_cook::project_rust_input_inventory,
    },
    project_workflow::cargo::CargoBuildSelection,
};

#[allow(dead_code)]
mod game;
mod process;
mod workspace;

use process::{Custody, Output, install, remove_owned_tree};
use workspace::{Project, SELECTED_SCHEMA};

const COOKED_DELIVERY_NAME: &str = "IndependentGame";
const WORKER_ROOT: &str = "NARA_PROJECT_COOK_ACCEPTANCE_ROOT";
const TEST: &str = "generated_project_cooks_custom_game_and_runs_after_relocation";

pub fn accept_timeout_cleanup() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let target = baseline(&checkout);
    let mut custody = Custody::create(&target);
    custody.assert_live_timeout_cleanup();
}

pub fn accept() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let target = baseline(&checkout);
    if let Some(root) = std::env::var_os(WORKER_ROOT) {
        worker(&checkout, &target, PathBuf::from(root));
        return;
    }
    let mut custody = Custody::create(&target);
    let command = SupervisedProcessCommand::new(std::env::current_exe().unwrap(), &custody.root)
        .arguments([
            "--exact",
            TEST,
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .environment(WORKER_ROOT, &custody.root);
    let output = custody.tool(command, Duration::from_secs(1800));
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    output.assert_success();
    assert!(!custody.root.join(process::UNRESOLVED).exists());
    assert!(output.stdout.lines().any(|line| line
        == "PROJECT_COOK_ACCEPTANCE source-free custom-schema third-scene new-image passed"));
}

fn baseline(checkout: &Path) -> PathBuf {
    let toolchain =
        std::env::var("RUSTUP_TOOLCHAIN").expect("run qualification with RUSTUP_TOOLCHAIN=1.95.0");
    assert!(toolchain == "1.95.0" || toolchain.starts_with("1.95.0-"));
    assert_eq!(std::env::var("CARGO_NET_OFFLINE").as_deref(), Ok("true"));
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let target = PathBuf::from(
        std::env::var_os("CARGO_TARGET_DIR")
            .expect("set the absolute root CARGO_TARGET_DIR; run this ignored test serially"),
    );
    assert!(target.is_absolute());
    let target = target.canonicalize().unwrap();
    assert_eq!(target, checkout.join("target").canonicalize().unwrap());
    target
}

fn worker(checkout: &Path, target: &Path, root: PathBuf) {
    let mut custody = Custody::worker(root, target);
    let project = Project::create(checkout, &custody.root);
    project.author();
    let tools = CargoBuildTools::discover_in(&project.root).unwrap();
    let identity = custody.tool(
        tool_command(tools.rustc_path(), &project.root, &tools, target).argument("-vV"),
        Duration::from_secs(30),
    );
    identity.assert_success();
    assert!(
        identity
            .stdout
            .lines()
            .any(|line| line == "release: 1.95.0"),
        "{}",
        identity.stdout
    );
    let host = identity
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let initialize = tool_command(tools.cargo_path(), &project.root, &tools, target)
        .arguments(["generate-lockfile", "--offline", "--manifest-path"])
        .argument(project.root.join("Cargo.toml"));
    custody
        .tool(initialize, Duration::from_secs(120))
        .assert_success();
    let installed_tools = custody.directory("captured-tools");
    let headless = build(
        &mut custody,
        &project,
        target,
        &tools,
        host,
        "headless",
        &installed_tools,
    );
    let cooker = build(
        &mut custody,
        &project,
        target,
        &tools,
        host,
        "cook",
        &installed_tools,
    );
    let output_parent = custody.directory("cook-output");
    let initial = project_tool(
        &mut custody,
        &headless,
        &project.root,
        &tools,
        target,
        "check",
    );
    let initial = tool_response(&initial, "check", false);
    assert!(
        initial["failure"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| { entry["code"] == "project.runtime-agreement.selected-owner-missing" })
    );
    let rejected = cook(
        &mut custody,
        &cooker,
        &project.root,
        &output_parent,
        target,
        &tools,
        host,
        "divergent-schema",
    );
    assert!(!rejected.receipt.exit().success());
    assert!(!rejected.stdout.contains("STARTER_PACKAGE_PUBLISHED"));
    assert!(
        rejected
            .stderr
            .contains("project.runtime-agreement.selected-owner-missing"),
        "{}",
        rejected.stderr
    );
    assert!(!output_parent.join("divergent-schema").exists());
    let refreshed = project_tool(
        &mut custody,
        &headless,
        &project.root,
        &tools,
        target,
        "schema-write",
    );
    let response = tool_response(&refreshed, "schema-write", true);
    assert_eq!(response["written"], true);
    let agreed = project_tool(
        &mut custody,
        &headless,
        &project.root,
        &tools,
        target,
        "schema-check",
    );
    assert_eq!(
        tool_response(&agreed, "schema-check", true)["written"],
        false
    );
    let valid_bundle = fs::read(project.root.join(SELECTED_SCHEMA)).unwrap();
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        &valid_bundle,
    )
    .unwrap();
    for (case, diagnostic) in [
        ("missing-schema", "project.content.host-io"),
        ("invalid-schema", "project.runtime-agreement.bundle-invalid"),
    ] {
        if case == "missing-schema" {
            fs::remove_file(project.root.join(SELECTED_SCHEMA)).unwrap();
        } else {
            fs::write(
                project.root.join(SELECTED_SCHEMA),
                b"{ invalid selected Schema",
            )
            .unwrap();
        }
        let checked = project_tool(
            &mut custody,
            &headless,
            &project.root,
            &tools,
            target,
            "check",
        );
        let failure = tool_response(&checked, "check", false);
        assert_eq!(failure["failure"]["stage"], "content_admission");
        assert!(
            failure["failure"]["diagnostics"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["code"] == diagnostic)
        );
        let rejected = cook(
            &mut custody,
            &cooker,
            &project.root,
            &output_parent,
            target,
            &tools,
            host,
            case,
        );
        assert!(
            !rejected.receipt.exit().success(),
            "invalid selected Schema must refuse Cook"
        );
        assert!(!rejected.stdout.contains("STARTER_PACKAGE_PUBLISHED"));
        assert!(rejected.stderr.contains(diagnostic), "{}", rejected.stderr);
        assert!(!output_parent.join(case).exists());
        assert!(
            fs::read_dir(&output_parent)
                .unwrap()
                .all(|entry| entry.unwrap().file_name() == "nara-cook-cache")
        );
        fs::write(project.root.join(SELECTED_SCHEMA), &valid_bundle).unwrap();
    }
    let checked = project_tool(
        &mut custody,
        &headless,
        &project.root,
        &tools,
        target,
        "check",
    );
    let checked = tool_response(&checked, "check", true);
    assert!(checked["content_members"].as_u64().unwrap() >= 3);
    project.assert_identity();
    let cooked = cook(
        &mut custody,
        &cooker,
        &project.root,
        &output_parent,
        target,
        &tools,
        host,
        COOKED_DELIVERY_NAME,
    );
    cooked.assert_success();
    assert!(
        !cooked.stdout.contains("PROJECT_COOK_GAMEPLAY"),
        "Cook must not start gameplay"
    );
    assert_eq!(
        cooked
            .stdout
            .lines()
            .filter(|line| line.starts_with("STARTER_PACKAGE_PUBLISHED "))
            .count(),
        1
    );
    let published = output_parent
        .join(COOKED_DELIVERY_NAME)
        .canonicalize()
        .unwrap();
    let original = payload(&published);
    let package = host_fs::readable_file(&published.join("package.json"))
        .unwrap()
        .read_to_end_bounded(1024 * 1024)
        .unwrap();
    let package: serde_json::Value = serde_json::from_slice(&package).unwrap();
    assert_eq!(package["payload"]["pack_file"], "content.pack");
    assert_eq!(package["payload"]["content_target"]["triple"], host);
    assert_eq!(
        package["payload"]["content_target"]["profile"],
        "game-shipping"
    );
    assert_eq!(
        package["payload"]["build"]["selection"]["package"],
        project.cargo_package_name.as_str()
    );
    assert_eq!(package["payload"]["build"]["selection"]["bin"], "player");
    assert_eq!(
        package["payload"]["build"]["selection"]["cargo_profile"],
        "dev"
    );
    let relocated = custody.root.join("relocated complete installation");
    assert!(published.starts_with(&custody.root) && relocated.starts_with(&custody.root));
    assert!(!relocated.exists());
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    assert_eq!(payload(&relocated), original);
    let source = project.root.clone();
    drop(project);
    remove_owned_tree(&custody.root, &source);
    remove_owned_tree(&custody.root, &output_parent);
    remove_owned_tree(&custody.root, &installed_tools);
    assert!(!source.exists() && !output_parent.exists() && !installed_tools.exists());
    let cwd = custody.directory("unrelated empty working directory");
    let executable = relocated.join(format!("player{}", std::env::consts::EXE_SUFFIX));
    let played = custody.player(&executable, &cwd);
    played.assert_success();
    assert_gameplay(&played.stdout);
    assert!(fs::read_dir(&cwd).unwrap().next().is_none());
    assert_eq!(payload(&relocated), original);
    print!("{}", played.stdout);
    eprint!("{}", played.stderr);
    println!("PROJECT_COOK_ACCEPTANCE source-free custom-schema third-scene new-image passed");
}

fn tool_command(
    executable: &Path,
    cwd: &Path,
    tools: &CargoBuildTools,
    target: &Path,
) -> SupervisedProcessCommand {
    SupervisedProcessCommand::new(executable, cwd)
        .environment("CARGO", tools.cargo_path())
        .environment("RUSTC", tools.rustc_path())
        .environment("RUSTUP_TOOLCHAIN", "1.95.0")
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .environment("CARGO_NET_OFFLINE", "true")
        .environment("CARGO_BUILD_JOBS", "1")
        .environment("CARGO_TARGET_DIR", target)
}

fn build(
    custody: &mut Custody,
    project: &Project,
    target: &Path,
    tools: &CargoBuildTools,
    host: &str,
    bin: &str,
    installed: &Path,
) -> PathBuf {
    let selection = CargoBuildSelection::new(
        project.cargo_package_name.as_str(),
        bin,
        "dev",
        host,
        std::iter::empty::<&str>(),
        false,
    )
    .unwrap();
    let request = CargoBuildRequest::new(
        &project.root,
        RelativePath::new("Cargo.toml").unwrap(),
        selection,
        tools.clone(),
        project_rust_input_inventory(&project.root).unwrap(),
        CargoBuildLimits::default(),
    )
    .unwrap()
    .with_target_directory(target)
    .unwrap();
    let captured = custody.build(&request);
    assert_eq!(captured.provenance().toolchain().host_target(), host);
    install(captured, installed, bin)
}

fn project_tool(
    custody: &mut Custody,
    headless: &Path,
    project: &Path,
    tools: &CargoBuildTools,
    target: &Path,
    operation: &str,
) -> Output {
    custody.tool(
        tool_command(headless, project, tools, target)
            .arguments(["--nara-project-tool", "1", operation, "--project-root"])
            .argument(project),
        Duration::from_secs(60),
    )
}

fn tool_response(output: &Output, operation: &str, success: bool) -> serde_json::Value {
    assert!(
        !output.stdout.contains("PROJECT_COOK_GAMEPLAY"),
        "Schema and Check must not start gameplay"
    );
    let response: serde_json::Value = serde_json::from_str(&output.stdout).unwrap();
    assert_eq!(response["version"], 1);
    assert_eq!(response["operation"], operation);
    assert_eq!(
        response["success"], success,
        "{response}; stderr={}",
        output.stderr
    );
    assert_eq!(output.receipt.exit().success(), success);
    response
}

fn cook(
    custody: &mut Custody,
    cooker: &Path,
    project: &Path,
    output: &Path,
    target: &Path,
    tools: &CargoBuildTools,
    host: &str,
    name: &str,
) -> Output {
    let arguments = [
        OsString::from("--project-root"),
        project.as_os_str().to_owned(),
        OsString::from("--output-parent"),
        output.as_os_str().to_owned(),
        OsString::from("--name"),
        OsString::from(name),
        OsString::from("--target-dir"),
        target.as_os_str().to_owned(),
        OsString::from("--target"),
        OsString::from(host),
        OsString::from("--cargo"),
        tools.cargo_path().as_os_str().to_owned(),
        OsString::from("--rustc"),
        tools.rustc_path().as_os_str().to_owned(),
        OsString::from("--profile"),
        OsString::from("dev"),
    ];
    custody.tool(
        tool_command(cooker, project, tools, target).arguments(arguments),
        Duration::from_secs(900),
    )
}

fn payload(directory: &Path) -> BTreeMap<String, ContentDigest> {
    let mut payload = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let name = entry.file_name().into_string().unwrap();
        let digest = host_fs::readable_file(&entry.path())
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap();
        assert!(digest.length() != 0);
        assert!(payload.insert(name, digest).is_none());
    }
    let mut expected = vec![
        format!("player{}", std::env::consts::EXE_SUFFIX),
        "content.pack".to_owned(),
        "package.json".to_owned(),
    ];
    expected.sort();
    assert_eq!(payload.keys().cloned().collect::<Vec<_>>(), expected);
    payload
}

fn assert_gameplay(stdout: &str) {
    let actual = stdout
        .lines()
        .filter(|line| line.starts_with("PROJECT_COOK_GAMEPLAY "))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        [
            "PROJECT_COOK_GAMEPLAY scene=startup authored=17 computed=122 image=818822b5-b34f-4ea0-b837-39f73827182b/primary extent=1x1 pixels=[17, 39, 91, 255]",
            "PROJECT_COOK_GAMEPLAY scene=second authored=43 computed=304 image=b771dc9c-fb09-4f39-b1a4-6b997e8df008/primary extent=2x1 pixels=[47, 151, 23, 255, 47, 151, 23, 255]",
            "PROJECT_COOK_GAMEPLAY scene=third authored=97 computed=682 image=ee0e8d7c-b034-4bde-9077-fc8d4b478746/primary extent=3x1 pixels=[201, 73, 119, 255, 201, 73, 119, 255, 201, 73, 119, 255]",
        ]
    );
    assert_eq!(
        stdout
            .lines()
            .filter(|line| *line == "PROJECT_COOK_COMPLETE scenes=3 travels=2 score=1108")
            .count(),
        1
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("starter_player: ok frame_limit=4096 fixed_ticks="))
    );
}
