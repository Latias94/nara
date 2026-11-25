use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    fs::{ContentDigest, RelativePath},
    project_tooling::{
        ProjectToolFailure, ProjectToolFailureStage, ProjectToolInvocationError,
        ProjectToolRequest, ProjectToolSummary,
        cargo_build::{CargoBuildLimits, CargoBuildRequest, CargoBuildTools},
        invoke_project_tool,
        process::SupervisedProcessCommand,
        project_cook::project_rust_input_inventory,
    },
    project_workflow::cargo::CargoBuildSelection,
};

#[allow(dead_code)]
#[path = "../project_cook/process.rs"]
mod process;
pub(super) mod puzzle;
mod workspace;

use process::{Custody, install, remove_owned_tree};
use workspace::Project;

pub fn accept() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    assert_eq!(std::env::var("CARGO_NET_OFFLINE").as_deref(), Ok("true"));
    assert!(
        std::env::var("RUSTUP_TOOLCHAIN")
            .unwrap()
            .starts_with("1.95.0")
    );
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(target, checkout.join("target").canonicalize().unwrap());
    let mut custody = Custody::create(checkout.parent().unwrap());
    assert!(!custody.root.starts_with(&checkout));
    let first = Project::create(&checkout, &custody.directory("author one"));
    let second = Project::create(&checkout, &custody.directory("author two"));
    assert_eq!(first.manifest.project.name, second.manifest.project.name);
    assert_ne!(
        first.manifest.project.source_universe,
        second.manifest.project.source_universe
    );
    assert_ne!(first.cargo_package_name(), second.cargo_package_name());
    assert_ne!(first.startup, second.startup);
    assert_ne!(first.prefab, second.prefab);
    let tools = CargoBuildTools::discover_in(&first.root).unwrap();
    let compiler = custody.tool(
        command(tools.rustc_path(), &first.root, &tools, &target).argument("-vV"),
        Duration::from_secs(30),
    );
    compiler.assert_success();
    assert!(
        compiler
            .stdout
            .lines()
            .any(|line| line == "release: 1.95.0")
    );
    let host = compiler
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap()
        .to_owned();
    let cwd = custody.directory("unrelated empty cwd");
    let mut installations = Vec::new();

    for (number, project) in [first, second].into_iter().enumerate() {
        project.add_first_system();
        check(&project.root, None).unwrap();
        let first_tools = custody.directory(&format!("first-tools-{number}"));
        let headless = build(
            &mut custody,
            &project,
            &tools,
            &target,
            &host,
            "headless",
            &first_tools,
        );
        let first_run = custody.tool(
            command(&headless, &cwd, &tools, &target),
            Duration::from_secs(60),
        );
        first_run.assert_success();
        assert!(
            first_run
                .stdout
                .lines()
                .any(|line| line == "first_fixed_step: tick=1")
        );
        assert!(
            first_run
                .stdout
                .contains("starter_headless: ok fixed_ticks=1")
        );
        project.assert_identity();

        project.add_collection(&checkout);
        let updated = custody.tool(
            command(tools.cargo_path(), &project.root, &tools, &target)
                .arguments(["generate-lockfile", "--offline", "--manifest-path"])
                .argument(project.root.join("Cargo.toml")),
            Duration::from_secs(120),
        );
        updated.assert_success();
        let stale = project_failure(schema(&project.root, None, false).unwrap_err());
        assert_eq!(stale.code(), "project.tool.schema-stale");
        assert!(schema(&project.root, None, true).unwrap());
        assert!(!schema(&project.root, None, false).unwrap());
        check(&project.root, None).unwrap();

        let prefab_meta = project.root.join("prefabs/player.prefab.json.meta");
        let prefab_bytes = fs::read(&prefab_meta).unwrap();
        fs::remove_file(&prefab_meta).unwrap();
        let missing = project_failure(check(&project.root, None).unwrap_err());
        assert_eq!(missing.stage(), ProjectToolFailureStage::ContentAdmission);
        fs::write(&prefab_meta, &prefab_bytes).unwrap();
        check(&project.root, None).unwrap();
        let authored = project.authored_bytes();

        let captured_tools = custody.directory(&format!("captured-tools-{number}"));
        let headless = build(
            &mut custody,
            &project,
            &tools,
            &target,
            &host,
            "headless",
            &captured_tools,
        );
        for reset in [false, true] {
            let mut invocation = command(&headless, &cwd, &tools, &target);
            if reset {
                invocation = invocation.argument("--verify-reset");
            }
            let played = custody.tool(invocation, Duration::from_secs(60));
            played.assert_success();
            assert!(played.stdout.contains("first_fixed_step: tick=1"));
            if reset {
                assert!(
                    played.stdout.contains(
                        "tick=66 collected=0 targets=3 won=false resets=1 player=(-120.0,0.0)"
                    ),
                    "{}",
                    played.stdout
                );
            } else {
                assert!(
                    played
                        .stdout
                        .contains("tick=62 collected=3 targets=3 won=true resets=0"),
                    "{}",
                    played.stdout
                );
            }
            assert_eq!(project.authored_bytes(), authored);
        }
        let cooker = build(
            &mut custody,
            &project,
            &tools,
            &target,
            &host,
            "cook",
            &captured_tools,
        );
        let output = custody.directory(&format!("cook-output-{number}"));
        let cooked = custody.tool(
            command(&cooker, &cwd, &tools, &target).arguments([
                OsString::from("--project-root"),
                project.root.as_os_str().to_owned(),
                OsString::from("--output-parent"),
                output.as_os_str().to_owned(),
                OsString::from("--name"),
                OsString::from("collection-player"),
                OsString::from("--target-dir"),
                target.as_os_str().to_owned(),
                OsString::from("--target"),
                OsString::from(&host),
                OsString::from("--cargo"),
                tools.cargo_path().as_os_str().to_owned(),
                OsString::from("--rustc"),
                tools.rustc_path().as_os_str().to_owned(),
                OsString::from("--profile"),
                OsString::from("dev"),
            ]),
            Duration::from_secs(900),
        );
        cooked.assert_success();
        assert_eq!(
            cooked
                .stdout
                .lines()
                .filter(|line| line.starts_with("STARTER_PACKAGE_PUBLISHED "))
                .count(),
            1
        );
        assert!(!cooked.stdout.contains("first_fixed_step:"));
        project.assert_identity();
        assert_eq!(project.authored_bytes(), authored);
        assert_eq!(fs::read(prefab_meta).unwrap(), prefab_bytes);
        let relocated = custody.root.join(format!("relocated-player-{number}"));
        fs::rename(output.join("collection-player"), &relocated).unwrap();
        let pack = fs::read(relocated.join("content.pack")).unwrap();
        let executable = relocated.join(format!("player{}", std::env::consts::EXE_SUFFIX));
        assert_eq!(fs::read_dir(&relocated).unwrap().count(), 3);
        let source = project.root.clone();
        drop(project);
        remove_owned_tree(&custody.root, &source);
        remove_owned_tree(&custody.root, &output);
        remove_owned_tree(&custody.root, &captured_tools);
        remove_owned_tree(&custody.root, &first_tools);
        assert!(!source.exists() && !output.exists());
        let played = custody.player(&executable, &cwd);
        played.assert_success();
        assert!(played.stdout.contains("first_fixed_step: tick=1"));
        assert!(
            played
                .stdout
                .contains("starter_player: ok frame_limit=4096 fixed_ticks=4096"),
            "{}",
            played.stdout
        );
        assert_eq!(fs::read(relocated.join("content.pack")).unwrap(), pack);
        assert!(fs::read_dir(&cwd).unwrap().next().is_none());
        installations.push((relocated, executable, pack));
    }

    assert_ne!(
        ContentDigest::of_bytes(&installations[0].2),
        ContentDigest::of_bytes(&installations[1].2)
    );
    fs::write(installations[1].0.join("content.pack"), &installations[0].2).unwrap();
    let rejected = custody.player(&installations[1].1, &cwd);
    assert!(!rejected.receipt.exit().success());
    assert_eq!(rejected.receipt.exit().code(), Some(1));
    assert!(
        rejected
            .stderr
            .contains("Runtime catalog differs from the Host expectation")
            || rejected
                .stderr
                .contains("Runtime pack header differs from the selected format or catalog length"),
        "{}",
        rejected.stderr
    );
    assert!(
        !rejected.stdout.contains("first_fixed_step:")
            && !rejected.stdout.contains("starter_player: ok")
    );
    fs::write(installations[1].0.join("content.pack"), &installations[1].2).unwrap();
    let mut changed = installations[1].2.clone();
    let catalog_length =
        usize::try_from(u64::from_le_bytes(changed[12..20].try_into().unwrap())).unwrap();
    let end = 20 + catalog_length;
    let needle = b"game-shipping";
    let start = changed[20..end]
        .windows(needle.len())
        .position(|value| value == needle)
        .unwrap()
        + 20;
    changed[start + needle.len() - 1] = b'h';
    assert_eq!(&changed[..20], &installations[1].2[..20]);
    assert_eq!(&changed[end..], &installations[1].2[end..]);
    serde_json::from_slice::<serde_json::Value>(&changed[20..end]).unwrap();
    fs::write(installations[1].0.join("content.pack"), &changed).unwrap();
    let refused = custody.player(&installations[1].1, &cwd);
    assert_eq!(refused.receipt.exit().code(), Some(1));
    assert!(
        refused
            .stderr
            .contains("Runtime catalog differs from the Host expectation"),
        "{}",
        refused.stderr
    );
    assert!(
        !refused.stdout.contains("first_fixed_step:")
            && !refused.stdout.contains("starter_player: ok")
    );
    fs::write(installations[1].0.join("content.pack"), &installations[1].2).unwrap();
    custody.player(&installations[1].1, &cwd).assert_success();
    println!(
        "AUTHOR_WORKFLOW_OK same_slug=2 distinct_identities=true source_win_reset=2 relocated_players=2 cross_pack_refused=true"
    );
}

pub fn missing_metadata_location() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    let target = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(target, checkout.join("target").canonicalize().unwrap());
    let mut custody = Custody::create(checkout.parent().unwrap());
    let project = Project::create(&checkout, &custody.directory("metadata location"));
    project.add_first_system();
    project.add_collection(&checkout);
    let tools = CargoBuildTools::discover_in(&project.root).unwrap();
    custody
        .tool(
            command(tools.cargo_path(), &project.root, &tools, &target)
                .arguments(["generate-lockfile", "--offline", "--manifest-path"])
                .argument(project.root.join("Cargo.toml")),
            Duration::from_secs(120),
        )
        .assert_success();
    assert!(schema(&project.root, None, true).unwrap());
    check(&project.root, None).unwrap();
    let relative = "prefabs/player.prefab.json.meta";
    let metadata = project.root.join(relative);
    let bytes = fs::read(&metadata).unwrap();
    fs::remove_file(&metadata).unwrap();
    let failure = project_failure(check(&project.root, None).unwrap_err());
    assert_eq!(failure.stage(), ProjectToolFailureStage::ContentAdmission);
    let diagnostic = failure
        .diagnostics()
        .iter()
        .find(|entry| entry.code() == "project.content.host-io")
        .expect("the original I/O diagnostic must reach the public caller");
    assert_eq!(diagnostic.project_path(), Some(relative));
    assert_eq!(diagnostic.summary(), "Project content host I/O failed");
    let displayed = failure.to_string();
    assert!(displayed.contains(relative), "{displayed}");
    assert!(
        !displayed.contains(project.root.to_str().unwrap()),
        "{displayed}"
    );
    fs::write(&metadata, bytes).unwrap();
    check(&project.root, None).unwrap();
    project.assert_identity();
    println!(
        "METADATA_LOCATION_OK project_relative=true restored_check=true gameplay_started=false"
    );
}

fn command(
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

fn check(project: &Path, profile: Option<&str>) -> Result<(), ProjectToolInvocationError> {
    // The compiled tool must return one structured response before the gameplay entry runs.
    let result = invoke_project_tool(ProjectToolRequest::Check {
        project: project.to_path_buf(),
        profile: profile.map(str::to_owned),
    })?;
    assert!(
        matches!(
            result,
            ProjectToolSummary::Checked {
                content_members: 1..
            }
        ),
        "{result:?}"
    );
    Ok(())
}

fn schema(
    project: &Path,
    profile: Option<&str>,
    write: bool,
) -> Result<bool, ProjectToolInvocationError> {
    match invoke_project_tool(ProjectToolRequest::Schema {
        project: project.to_path_buf(),
        profile: profile.map(str::to_owned),
        write,
    })? {
        ProjectToolSummary::Schema { written } => Ok(written),
        summary => panic!("expected the schema outcome: {summary:?}"),
    }
}

fn project_failure(error: ProjectToolInvocationError) -> ProjectToolFailure {
    match error {
        ProjectToolInvocationError::Project(failure) => failure,
        error => panic!("expected a compiled project failure: {error:?}"),
    }
}

fn build(
    custody: &mut Custody,
    project: &Project,
    tools: &CargoBuildTools,
    target: &Path,
    host: &str,
    bin: &str,
    installed: &Path,
) -> PathBuf {
    let mut inputs = project_rust_input_inventory(&project.root).unwrap();
    if project.root.join("gameplay-plugin").exists() {
        inputs.extend(
            [
                "gameplay-plugin/Cargo.toml",
                "gameplay-plugin/src/lib.rs",
                "gameplay-plugin/src/input.rs",
                "gameplay-plugin/src/walking.rs",
                "gameplay-plugin/schema/component-schema-generation-1.json",
            ]
            .map(|name| RelativePath::new(name).unwrap()),
        );
    }
    let request = CargoBuildRequest::new(
        &project.root,
        RelativePath::new("Cargo.toml").unwrap(),
        CargoBuildSelection::new(
            project.cargo_package_name(),
            bin,
            "dev",
            host,
            std::iter::empty::<&str>(),
            false,
        )
        .unwrap(),
        tools.clone(),
        inputs,
        CargoBuildLimits::default(),
    )
    .unwrap()
    .with_target_directory(target)
    .unwrap();
    let captured = custody.build(&request);
    install(captured, installed, bin)
}
