//! Actual release Cook, relocated installation and sealed physical round-flow acceptance.

use std::{
    ffi::OsString,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    project::ProjectManifest,
    project_tooling::{
        cargo_build::CargoBuildTools,
        host_fs,
        process::{
            ExactExecutableLaunchAuthority, SupervisedProcessCommand, SupervisedProcessOwner,
            SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand,
        },
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use two_level_game::delivery::{ControllerBackend, GameFlow, PlayerSelection, RuntimeKind};

use crate::continuity::Fixture;

#[path = "cooked_round_flow/evidence.rs"]
mod evidence;
#[path = "cooked_round_flow/package.rs"]
mod package;
#[path = "cooked_desktop/process.rs"]
#[allow(dead_code)]
mod process;
#[path = "cooked_round_flow/window.rs"]
mod window;
#[path = "cooked_round_flow/workflow.rs"]
mod workflow;

use process::{NativeProcess, read_bounded};

const OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const REQUEST: &str = "cooked-round-request.json";
const WORKER: &str = "cooked_round_flow_sealed_player_worker";
const REPORT: &str = "TWO_LEVEL_COOKED_ROUND_ACCEPTANCE ";
const WORKER_DIRECTORY: &str = "sealed-round-worker";
const UNRESOLVED: &str = "unresolved-cleanup";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    executable: PathBuf,
    mismatched_executable: PathBuf,
    cwd: PathBuf,
    invalid_path: PathBuf,
    local_data: PathBuf,
    roaming_data: PathBuf,
    common_data: PathBuf,
    save: PathBuf,
    unavailable_source: PathBuf,
    unavailable_cache: PathBuf,
    unresolved_cleanup: PathBuf,
    title: String,
    target: String,
    content_profile: String,
    runtime: String,
    controllers: String,
    catalog_digest: Value,
}

struct Custody {
    fixture: Option<Fixture>,
    process: Option<NativeProcess>,
}

impl Custody {
    fn fixture(&self) -> &Fixture {
        self.fixture.as_ref().unwrap()
    }

    fn run(&mut self, process: NativeProcess, timeout: Duration, cooker: bool) -> String {
        assert!(
            self.process.is_none(),
            "nested Cargo and native operations are serial"
        );
        self.process = Some(process);
        let process = self.process.as_mut().unwrap();
        process.finish(timeout);
        if cooker {
            process.assert_cooker_clean();
        } else {
            process.assert_clean();
        }
        assert!(
            process.receipt().exit().success(),
            "{}",
            process.diagnostics()
        );
        let text = process.stdout();
        self.process.take();
        text
    }
}

impl Drop for Custody {
    fn drop(&mut self) {
        if let Some(process) = &mut self.process
            && !process.retire_normally(Duration::from_secs(30))
        {
            eprintln!(
                "retaining original round process and fixture: {}",
                process.diagnostics()
            );
            std::mem::forget(self.process.take());
            std::mem::forget(self.fixture.take());
        }
        if self.fixture.as_ref().is_some_and(|fixture| {
            fixture
                .root
                .join(WORKER_DIRECTORY)
                .join(UNRESOLVED)
                .exists()
        }) || self
            .process
            .as_ref()
            .is_some_and(|process| process.complete() && !process.receipt().exit().success())
        {
            eprintln!(
                "retaining owned round fixture after failed worker or unresolved original cleanup"
            );
            std::mem::forget(self.fixture.take());
        }
    }
}

pub fn accept(cooker: &Path, runtime: RuntimeKind, controllers: ControllerBackend) {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    assert!(matches!(
        runtime,
        RuntimeKind::Desktop | RuntimeKind::AudioDesktop
    ));
    let player = PlayerSelection::new(runtime, controllers)
        .unwrap()
        .with_flow(GameFlow::Round)
        .unwrap();
    let mut custody = Custody {
        fixture: Some(Fixture::new()),
        process: None,
    };
    let fixture = custody.fixture();
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    assert!(
        triple.ends_with("-pc-windows-msvc"),
        "actual Windows target: {triple}"
    );
    player.validate_target(&triple).unwrap();
    let command = SupervisedProcessCommand::new(tools.cargo_path(), &fixture.project)
        .arguments([
            "tree",
            "--locked",
            "--offline",
            "--target",
            &triple,
            "--no-default-features",
            "--features",
            &player.features().join(","),
            "--edges",
            "normal,build",
            "--prefix",
            "none",
            "--format",
            "{p}|{f}",
        ])
        .environment("CARGO_BUILD_JOBS", "1")
        .environment("CARGO_NET_OFFLINE", "true")
        .environment("RUSTUP_TOOLCHAIN", "1.95.0")
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .bounded_stdout(NonZeroUsize::new(256 * 1024).unwrap());
    let graph = custody.run(
        NativeProcess::spawn(&command, None),
        Duration::from_secs(60),
        true,
    );
    package::feature_closure(&graph, player);
    let fixture = custody.fixture();
    let loaded = ProjectManifest::parse_toml_bytes(fixture.read("nara.toml").as_bytes());
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    let settings = loaded
        .manifest
        .unwrap()
        .resolve_profile(player.manifest_profile())
        .unwrap();
    assert!(settings.window.enabled);
    assert_eq!(
        settings.input.action_map.as_ref().unwrap().as_str(),
        two_level_game::input::INPUT_PATH
    );
    let title = settings.window.title.clone();
    assert!(!title.is_empty() && title.len() < 1024);
    let mut configuration = settings
        .source_runtime_projection()
        .unwrap()
        .runtime()
        .to_configuration();
    configuration.input.action_map = Some(two_level_game::input::input_product());
    let output = fixture.directory("cook-output");
    let lock = read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024);
    let command = SupervisedProcessCommand::new(cooker, &fixture.root)
        .arguments([
            OsString::from("--project-root"),
            fixture.project.as_os_str().to_owned(),
            "--output-parent".into(),
            output.as_os_str().to_owned(),
            "--name".into(),
            "TwoLevelRound".into(),
            "--runtime".into(),
            runtime.name().into(),
            "--controllers".into(),
            controllers.name().into(),
            "--flow".into(),
            "round".into(),
            "--profile".into(),
            "release".into(),
            "--target-dir".into(),
            target.as_os_str().to_owned(),
            "--target".into(),
            triple.clone().into(),
            "--cargo".into(),
            tools.cargo_path().as_os_str().to_owned(),
            "--rustc".into(),
            tools.rustc_path().as_os_str().to_owned(),
        ])
        .environment("CARGO_BUILD_JOBS", "1")
        .environment("CARGO_NET_OFFLINE", "true")
        .environment("CARGO_TARGET_DIR", &target)
        .environment("RUSTUP_TOOLCHAIN", "1.95.0")
        .environment("RUSTUP_AUTO_INSTALL", "0")
        .environment("CARGO", tools.cargo_path())
        .environment("RUSTC", tools.rustc_path())
        .bounded_stdout(NonZeroUsize::new(OUTPUT_BYTES).unwrap());
    let cooked = custody.run(
        NativeProcess::spawn(&command, None),
        Duration::from_secs(30 * 60),
        true,
    );
    let fixture = custody.fixture();
    assert_eq!(
        read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024),
        lock
    );
    let publication = process::record(&cooked, "TWO_LEVEL_PACKAGE_PUBLISHED ");
    assert!(publication["postpublication_error"].is_null());
    assert_eq!(publication["runtime"], runtime.name());
    assert_eq!(publication["controllers"], controllers.name());
    assert_eq!(publication["flow"], "round");
    let executable_name = format!("{}.exe", player.binary());
    assert_eq!(publication["executable"], executable_name);
    let published = output.join("TwoLevelRound").canonicalize().unwrap();
    assert_eq!(
        Path::new(publication["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        published
    );
    let manifest_bytes = read_bounded(&published.join("package.json"), 1024 * 1024);
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let payload = &manifest["payload"];
    package::inspect(&published, payload, &triple, &configuration, player);
    assert!(!fixture.project.join("player-bootstrap.rs").exists());
    assert!(!fixture.project.join("src/bin/player/generated.rs").exists());
    let names = [executable_name.as_str(), "content.pack", "package.json"];
    let digests = names.map(|name| {
        host_fs::readable_file(&published.join(name))
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap()
    });
    let relocated = fixture
        .directory("relocated-installation")
        .join("TwoLevelRound");
    assert!(!relocated.exists());
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    let mismatched = fixture.directory("mismatched-installation");
    for name in names {
        fs::copy(relocated.join(name), mismatched.join(name)).unwrap();
    }
    package::mismatch_catalog(&mismatched.join("content.pack"), player.content_profile());
    let source = fixture.project.canonicalize().unwrap();
    let cache = output.join("two-level-cook-cache").canonicalize().unwrap();
    for root in [&source, &cache] {
        assert!(root.is_absolute() && root.starts_with(&fixture.root) && root != &fixture.root);
        assert!(root == &fixture.project || root == &output.join("two-level-cook-cache"));
        assert!(!target.starts_with(root) && !fixture.repository.starts_with(root));
        process::remove_checked_tree(root);
        assert!(!root.exists());
    }
    let worker = fixture.directory(WORKER_DIRECTORY);
    let cwd = worker.join("empty");
    fs::create_dir(&cwd).unwrap();
    let request = Request {
        executable: relocated.join(&executable_name),
        mismatched_executable: mismatched.join(&executable_name),
        cwd,
        invalid_path: worker.join("no-cargo-or-rustc"),
        local_data: fixture.directory("player-local-data"),
        roaming_data: fixture.directory("player-roaming-data"),
        common_data: fixture.directory("player-common-data"),
        save: fixture
            .directory("application-save")
            .join("checkpoint.json"),
        unavailable_source: source,
        unavailable_cache: cache,
        unresolved_cleanup: worker.join(UNRESOLVED),
        title,
        target: triple,
        content_profile: player.content_profile().to_owned(),
        runtime: runtime.name().to_owned(),
        controllers: controllers.name().to_owned(),
        catalog_digest: payload["catalog_digest"].clone(),
    };
    fs::write(worker.join(REQUEST), serde_json::to_vec(&request).unwrap()).unwrap();
    let executable = std::env::current_exe().unwrap();
    let authority = ExactExecutableLaunchAuthority::from_verified_handles(
        host_fs::readable_file(&executable)
            .unwrap()
            .duplicate_restrictive_executable_host_handle()
            .unwrap(),
        host_fs::readable_directory(executable.parent().unwrap())
            .unwrap()
            .duplicate_restrictive_host_handle()
            .unwrap(),
    )
    .unwrap();
    let command = SupervisedProductProcessCommand::new(
        SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession,
        &worker,
    )
    .arguments([
        "--exact",
        WORKER,
        "--ignored",
        "--nocapture",
        "--test-threads=1",
    ]);
    let owner =
        SupervisedProcessOwner::spawn_exact(authority, &command, process::log_limits()).unwrap();
    let text = custody.run(
        NativeProcess::new(owner, None),
        Duration::from_secs(6 * 60),
        false,
    );
    let report = process::record(&text, REPORT);
    assert_eq!(report["success"], true);
    assert_eq!(report["runtime"], request.runtime);
    assert_eq!(report["controllers"], request.controllers);
    assert_eq!(report["physical_round_processes"], 1);
    assert_eq!(report["finite_processes"], 1);
    assert_eq!(report["refused_processes"], 2);
    println!("{REPORT}{report}");
    package::assert_payload(&relocated, &executable_name);
    for (name, digest) in names.into_iter().zip(digests) {
        assert_eq!(
            host_fs::readable_file(&relocated.join(name))
                .unwrap()
                .digest(512 * 1024 * 1024)
                .unwrap(),
            digest
        );
    }
    assert_eq!(
        read_bounded(&relocated.join("package.json"), 1024 * 1024),
        manifest_bytes
    );
    assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
    assert!(!request.unresolved_cleanup.exists());
    assert!(target.is_dir() && custody.fixture().repository.join("Cargo.toml").is_file());
    assert!(fs::read_dir(&request.cwd).unwrap().next().is_none());
}

pub fn worker() {
    let policy = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
    for (name, _) in std::env::vars_os() {
        assert!(
            policy
                .allowlisted_names()
                .iter()
                .any(|allowed| name.to_string_lossy().eq_ignore_ascii_case(allowed)),
            "unsealed round worker environment: {name:?}"
        );
    }
    let request: Request = serde_json::from_slice(&read_bounded(
        &std::env::current_dir().unwrap().join(REQUEST),
        64 * 1024,
    ))
    .unwrap();
    for path in [
        &request.executable,
        &request.mismatched_executable,
        &request.cwd,
        &request.invalid_path,
        &request.save,
        &request.local_data,
        &request.roaming_data,
        &request.common_data,
        &request.unavailable_source,
        &request.unavailable_cache,
        &request.unresolved_cleanup,
    ] {
        assert!(path.is_absolute());
    }
    assert!(!request.invalid_path.exists() && !request.save.exists());
    assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
    workflow::run(&request);
}
