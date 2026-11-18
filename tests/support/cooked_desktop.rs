use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    fs::RelativePath,
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
use serde_json::{Value, json};
use two_level_game::{
    animation::atlas_product,
    levels::{LevelId, LevelKey},
    markers::marker_image_product,
};

use crate::continuity::Fixture;

#[path = "cooked_desktop/evidence.rs"]
mod evidence;
#[path = "cooked_audio/images.rs"]
mod images;
#[path = "cooked_desktop/process.rs"]
mod process;
#[path = "cooked_desktop/walls.rs"]
mod walls;
#[path = "cooked_desktop/window.rs"]
mod window;
#[path = "cooked_desktop/workflow.rs"]
mod workflow;

use process::{NativeProcess, read_bounded};

const OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const REQUEST: &str = "cooked-desktop-request.json";
const WORKER: &str = "cooked_desktop_sealed_player_worker";
const REPORT: &str = "TWO_LEVEL_DESKTOP_ACCEPTANCE ";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    executable: PathBuf,
    cwd: PathBuf,
    invalid_path: PathBuf,
    local_data: PathBuf,
    roaming_data: PathBuf,
    common_data: PathBuf,
    save: PathBuf,
    title: String,
    target: String,
    catalog_digest: Value,
    images: BTreeMap<LevelId, Value>,
}

struct Custody {
    fixture: Option<Fixture>,
    process: Option<NativeProcess>,
}

enum Operation {
    Cooker,
    PlayerWorker,
}

impl Custody {
    fn fixture(&self) -> &Fixture {
        self.fixture.as_ref().unwrap()
    }

    fn run(&mut self, process: NativeProcess, timeout: Duration, operation: Operation) -> String {
        assert!(
            self.process.is_none(),
            "native operations and nested Cargo are serial"
        );
        self.process = Some(process);
        let process = self.process.as_mut().unwrap();
        process.finish(timeout);
        match operation {
            Operation::Cooker => process.assert_cooker_clean(),
            Operation::PlayerWorker => process.assert_clean(),
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
                "retaining unresolved original process and owned fixture: {}",
                process.diagnostics()
            );
            std::mem::forget(self.process.take());
            std::mem::forget(self.fixture.take());
        }
    }
}

pub fn accept(cooker: &Path) {
    assert_eq!(std::env::var("CARGO_BUILD_JOBS").as_deref(), Ok("1"));
    let mut custody = Custody {
        fixture: Some(Fixture::new()),
        process: None,
    };
    let fixture = custody.fixture();
    fixture.assert_player_feature_closure(true);
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    assert!(
        triple.ends_with("-pc-windows-msvc"),
        "actual Windows host triple: {triple}"
    );
    let loaded = ProjectManifest::parse_toml_bytes(fixture.read("nara.toml").as_bytes());
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    let settings = loaded
        .manifest
        .unwrap()
        .resolve_profile(Some("desktop"))
        .unwrap();
    assert!(settings.window.enabled);
    assert_eq!(
        settings.input.action_map.as_ref().unwrap().as_str(),
        two_level_game::input::INPUT_PATH
    );
    let mut source_input: Value =
        serde_json::from_str(&fixture.read(two_level_game::input::INPUT_PATH)).unwrap();
    let movement_binding = source_input["payload"]["bindings"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|binding| binding["action"] == "two-level-game.move-d")
        .unwrap();
    assert_eq!(
        movement_binding["input"],
        json!({"Key": {"Character": "d"}})
    );
    movement_binding["input"]["Key"]["Character"] = json!("j");
    fixture.write(
        two_level_game::input::INPUT_PATH,
        &serde_json::to_string(&source_input).unwrap(),
    );
    let title = settings.window.title.clone();
    assert!(!title.is_empty() && title.len() < 1024);
    let projection = settings.source_runtime_projection().unwrap();
    let selected = nara::input::ActionMapFileCandidate::decode_json_bytes(
        &fixture.read(two_level_game::input::INPUT_PATH).into_bytes(),
    )
    .unwrap();
    assert_eq!(selected.product(), two_level_game::input::input_product());
    let mut configuration = projection.runtime().to_configuration();
    configuration.input.action_map = Some(selected.product());
    let output = fixture.directory("cook-output");
    let lock = read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024);
    assert!(fixture.project.join("build.rs").is_file());
    let command = SupervisedProcessCommand::new(cooker, &fixture.root)
        .arguments([
            OsString::from("--project-root"),
            fixture.project.as_os_str().to_owned(),
            "--output-parent".into(),
            output.as_os_str().to_owned(),
            "--name".into(),
            "TwoLevelDesktop".into(),
            "--runtime".into(),
            "desktop".into(),
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
        .bounded_stdout(std::num::NonZeroUsize::new(OUTPUT_BYTES).unwrap());
    let cooked = custody.run(
        NativeProcess::spawn(&command, None),
        Duration::from_secs(30 * 60),
        Operation::Cooker,
    );
    let fixture = custody.fixture();
    assert_eq!(
        read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024),
        lock
    );
    let publication = process::record(&cooked, "TWO_LEVEL_PACKAGE_PUBLISHED ");
    assert!(publication["postpublication_error"].is_null());
    assert_eq!(publication["runtime"], "desktop");
    assert_eq!(publication["executable"], "player_desktop.exe");
    let published = output.join("TwoLevelDesktop").canonicalize().unwrap();
    assert_eq!(
        Path::new(publication["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        published
    );
    evidence::assert_payload(&published);
    let package_digests = ["player_desktop.exe", "content.pack", "package.json"].map(|name| {
        host_fs::readable_file(&published.join(name))
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap()
    });
    let package_bytes = read_bounded(&published.join("package.json"), 1024 * 1024);
    let manifest: Value = serde_json::from_slice(&package_bytes).unwrap();
    let payload = &manifest["payload"];
    let selection = &payload["build"]["selection"];
    assert_eq!(selection["package"], "two-level-game");
    assert_eq!(selection["bin"], "player_desktop");
    assert_eq!(selection["cargo_profile"], "release");
    assert_eq!(selection["target_triple"], triple);
    assert_eq!(selection["default_features"], false);
    assert_eq!(
        selection["requested_features"],
        json!(["cooked-player", "desktop"])
    );
    let images = evidence::inspect_package(&published, payload, &triple, &configuration);
    let bootstrap = payload["generated_input_name"].as_str().unwrap();
    assert!(RelativePath::new(bootstrap).is_ok());
    assert_eq!(bootstrap, "player-bootstrap.rs");
    assert!(!fixture.project.join(bootstrap).exists());
    assert!(!fixture.project.join("src/bin/player/generated.rs").exists());
    let generated_input = &payload["build"]["generated_rust_input"];
    assert_eq!(generated_input["path"], bootstrap);
    assert_eq!(
        generated_input["digest"][0],
        payload["generated_input_digest"]["length"]
    );
    assert_eq!(
        generated_input["digest"][1],
        payload["generated_input_digest"]["hash"]
    );
    let inputs = payload["build"]["declared_inputs"].as_array().unwrap();
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/gameplay/mod.rs",
        "src/gameplay/input.rs",
        "src/animation.rs",
        "src/bin/player_desktop.rs",
        "src/bin/player_desktop/arguments.rs",
        "src/bin/player_desktop/runtime.rs",
        "src/bin/player_desktop/presentation.rs",
        "src/bin/player_desktop/output.rs",
        "src/bin/player_desktop/verification.rs",
        "src/bin/player/evidence.rs",
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == required),
            "missing actual build input: {required}"
        );
    }
    assert!(inputs.iter().all(|input| input["path"] != bootstrap));
    let relocated = fixture
        .directory("relocated-installation")
        .join("TwoLevelDesktop");
    assert!(!relocated.exists());
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    let project = fixture.project.canonicalize().unwrap();
    let cache = output.join("two-level-cook-cache").canonicalize().unwrap();
    for root in [&project, &cache] {
        assert!(root == &fixture.project || root == &output.join("two-level-cook-cache"));
        assert!(root.starts_with(&fixture.root) && !target.starts_with(root));
        assert!(!fixture.repository.starts_with(root));
        process::remove_checked_tree(root);
        assert!(!root.exists());
    }
    let worker = fixture.directory("sealed-desktop-worker");
    let cwd = worker.join("empty");
    fs::create_dir(&cwd).unwrap();
    let request = Request {
        executable: relocated.join("player_desktop.exe"),
        cwd,
        invalid_path: worker.join("no-cargo-or-rustc"),
        local_data: fixture.directory("player-local-data"),
        roaming_data: fixture.directory("player-roaming-data"),
        common_data: fixture.directory("player-common-data"),
        save: fixture
            .directory("application-save")
            .join("checkpoint.json"),
        title,
        target: triple,
        catalog_digest: payload["catalog_digest"].clone(),
        images,
    };
    fs::write(worker.join(REQUEST), serde_json::to_vec(&request).unwrap()).unwrap();
    let executable = std::env::current_exe().unwrap();
    let file = host_fs::readable_file(&executable).unwrap();
    let directory = host_fs::readable_directory(executable.parent().unwrap()).unwrap();
    let authority = ExactExecutableLaunchAuthority::from_verified_handles(
        file.duplicate_restrictive_executable_host_handle().unwrap(),
        directory.duplicate_restrictive_host_handle().unwrap(),
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
        Operation::PlayerWorker,
    );
    let report = process::record(&text, REPORT);
    assert_eq!(report["success"], true);
    assert_eq!(report["authored_key"], "j");
    evidence::collision_excursions(&report);
    assert_eq!(report["ordinary_processes"], 2);
    assert_eq!(report["finite_processes"], 1);
    assert_eq!(report["refused_processes"], 1);
    assert_eq!(report["scripted_controls"].as_array().unwrap().len(), 3);
    for field in ["saved", "continued", "finite"] {
        assert_images(&report[field], &request.images);
    }
    assert_eq!(report["saved"]["animation_frames"], json!([0, 1, 2, 3]));
    assert_eq!(report["continued"]["animation_frames"], json!([0]));
    for control in report["scripted_controls"].as_array().unwrap() {
        assert_images(&control["result"], &request.images);
        assert_eq!(
            control["result"]["animation_frames"],
            if control["scenario"] == "restore" {
                json!([0])
            } else {
                json!([0, 1, 2, 3])
            },
            "completed cooked desktop {} animation frames",
            control["scenario"]
        );
    }
    println!("{REPORT}{report}");
    evidence::assert_payload(&relocated);
    for (name, expected) in ["player_desktop.exe", "content.pack", "package.json"]
        .into_iter()
        .zip(package_digests)
    {
        assert_eq!(
            host_fs::readable_file(&relocated.join(name))
                .unwrap()
                .digest(512 * 1024 * 1024)
                .unwrap(),
            expected,
            "the installed {name} bytes changed during Player execution"
        );
    }
    assert_eq!(
        read_bounded(&relocated.join("package.json"), 1024 * 1024),
        package_bytes
    );
    let fixture = custody.fixture();
    assert!(!project.exists() && !cache.exists());
    assert!(target.is_dir() && fixture.repository.join("Cargo.toml").is_file());
    let working_files = fs::read_dir(&request.cwd)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert!(
        working_files.is_empty(),
        "unexpected current-directory writes: {working_files:?}"
    );
}

fn assert_images(record: &Value, level_images: &BTreeMap<LevelId, Value>) {
    let current: LevelKey = serde_json::from_value(record["level"].clone()).unwrap();
    let resident: BTreeSet<_> = [
        current.level().image_product(),
        atlas_product(),
        marker_image_product(),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        record["resident_images"],
        json!(resident.iter().map(ToString::to_string).collect::<Vec<_>>())
    );
    let visited: BTreeSet<LevelId> =
        serde_json::from_value(record["visited_levels"].clone()).unwrap();
    assert_eq!(
        visited.len(),
        record["visited_levels"].as_array().unwrap().len()
    );
    assert!(visited.contains(&LevelId::STARTUP) && visited.contains(&current.level()));
    let products: BTreeSet<_> = visited
        .iter()
        .map(|level| level.image_product())
        .chain([atlas_product(), marker_image_product()])
        .collect();
    assert_eq!(
        record["observed_image_products"],
        json!(products.iter().map(ToString::to_string).collect::<Vec<_>>())
    );
    let proofs = record["image_proofs"].as_array().unwrap();
    assert_eq!(proofs.len(), products.len());
    assert_eq!(
        json!(
            proofs
                .iter()
                .map(|proof| &proof["product"])
                .collect::<Vec<_>>()
        ),
        record["observed_image_products"]
    );
    for level in visited {
        assert_eq!(
            proofs
                .iter()
                .filter(|proof| **proof == level_images[&level])
                .count(),
            1
        );
    }
    let atlas = images::atlas_proof(&images::atlas());
    assert_eq!(proofs.iter().filter(|proof| **proof == atlas).count(), 1);
    let badge = images::marker_proof(&images::marker_badge());
    assert_eq!(proofs.iter().filter(|proof| **proof == badge).count(), 1);
}

pub fn worker() {
    let policy = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
    for (name, _) in std::env::vars_os() {
        assert!(
            policy
                .allowlisted_names()
                .iter()
                .any(|allowed| name.to_string_lossy().eq_ignore_ascii_case(allowed)),
            "unsealed environment name: {name:?}"
        );
    }
    let request: Request = serde_json::from_slice(&read_bounded(
        &std::env::current_dir().unwrap().join(REQUEST),
        64 * 1024,
    ))
    .unwrap();
    for path in [
        &request.executable,
        &request.cwd,
        &request.invalid_path,
        &request.save,
        &request.local_data,
        &request.roaming_data,
        &request.common_data,
    ] {
        assert!(path.is_absolute());
    }
    assert!(!request.invalid_path.exists() && !request.save.exists());
    workflow::run(&request);
}
