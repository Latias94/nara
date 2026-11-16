use std::{
    ffi::OsString,
    fs,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::Duration,
};

use nara::{
    fs::{ContentDigest, RelativePath},
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

use crate::continuity::Fixture;

mod evidence;
mod package;
#[path = "../cooked_desktop/process.rs"]
#[allow(dead_code)]
mod process;
#[path = "../sound_bank_content/wav.rs"]
mod wav;
#[path = "../cooked_desktop/window.rs"]
#[allow(dead_code)]
mod window;
mod workflow;

use process::{NativeProcess, read_bounded};

const OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const REQUEST: &str = "cooked-audio-request.json";
const WORKER: &str = "cooked_audio_sealed_player_worker";
const REPORT: &str = "TWO_LEVEL_COOKED_AUDIO_ACCEPTANCE ";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    executable: PathBuf,
    corrupt_executable: PathBuf,
    mismatched_executable: PathBuf,
    cwd: PathBuf,
    invalid_path: PathBuf,
    local_data: PathBuf,
    roaming_data: PathBuf,
    common_data: PathBuf,
    unavailable_source: PathBuf,
    unavailable_cache: PathBuf,
    unresolved_cleanup: PathBuf,
    title: String,
    target: String,
    catalog_digest: Value,
    source_samples: [Vec<f32>; 2],
}

struct Custody {
    fixture: Option<Fixture>,
    process: Option<NativeProcess>,
}

impl Custody {
    fn fixture(&self) -> &Fixture {
        self.fixture.as_ref().unwrap()
    }

    fn run(&mut self, process: NativeProcess, timeout: Duration) -> String {
        assert!(
            self.process.is_none(),
            "nested Cargo and native Players are serial"
        );
        self.process = Some(process);
        let process = self.process.as_mut().unwrap();
        process.finish(timeout);
        process.assert_clean();
        assert!(
            process.receipt().exit().success(),
            "{}",
            process.diagnostics()
        );
        let stdout = process.stdout();
        self.process.take();
        stdout
    }
}

impl Drop for Custody {
    fn drop(&mut self) {
        if let Some(process) = self.process.as_mut()
            && !process.retire_normally(Duration::from_secs(30))
        {
            eprintln!(
                "retaining unresolved cooked-audio process and copied source/cache: {}",
                process.diagnostics()
            );
            std::mem::forget(self.process.take());
            std::mem::forget(self.fixture.take());
        }
        if self.fixture.as_ref().is_some_and(|fixture| {
            fixture
                .root
                .join("sealed-audio-worker/unresolved-cleanup")
                .exists()
        }) || self
            .process
            .as_ref()
            .is_some_and(|process| process.complete() && !process.receipt().exit().success())
        {
            eprintln!(
                "retaining copied cooked-audio source/cache after failed acceptance or unresolved Player cleanup"
            );
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
    let target = fixture.repository.join("target").canonicalize().unwrap();
    let tools = CargoBuildTools::discover_in(&fixture.repository).unwrap();
    let triple = fixture.request().selection().target_triple().to_owned();
    assert!(
        triple.ends_with("-pc-windows-msvc"),
        "actual native host target: {triple}"
    );
    let loaded = ProjectManifest::parse_toml_bytes(fixture.read("nara.toml").as_bytes());
    assert!(!loaded.has_errors(), "{:?}", loaded.diagnostics);
    let settings = loaded
        .manifest
        .unwrap()
        .resolve_profile(Some("desktop"))
        .unwrap();
    assert!(settings.window.enabled && settings.input.action_map.is_none());
    let title = settings.window.title.clone();
    let configuration = settings.runtime_configuration().unwrap().to_configuration();
    let source_samples = ["pickup.wav", "travel.wav"].map(|name| {
        wav::decode(&read_bounded(
            &fixture.project.join("assets/audio").join(name),
            1024 * 1024,
        ))
    });
    assert_eq!(
        [source_samples[0].len(), source_samples[1].len()],
        [5760, 11520]
    );
    let audio = fixture.project.join("assets/audio").canonicalize().unwrap();
    assert!(audio.starts_with(&fixture.project) && audio.starts_with(&fixture.root));
    let renamed = audio.join("pickup-v2.wav");
    assert!(!renamed.exists());
    fs::rename(audio.join("pickup.wav"), &renamed).unwrap();
    assert!(!audio.join("pickup.wav").exists());
    let descriptor = audio.join("game.soundbank.json");
    let mut bank: Value = serde_json::from_slice(&read_bounded(&descriptor, 1024)).unwrap();
    bank["pickup"] = "audio/pickup-v2.wav".into();
    fs::write(&descriptor, serde_json::to_vec(&bank).unwrap()).unwrap();
    let output = fixture.directory("cook-output");
    let lock = read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024);
    let command = SupervisedProcessCommand::new(cooker, &fixture.root)
        .arguments([
            OsString::from("--project-root"),
            fixture.project.as_os_str().to_owned(),
            "--output-parent".into(),
            output.as_os_str().to_owned(),
            "--name".into(),
            "TwoLevelAudio".into(),
            "--runtime".into(),
            "audio-desktop".into(),
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
    );
    let fixture = custody.fixture();
    assert_eq!(
        read_bounded(&fixture.project.join("Cargo.lock"), 1024 * 1024),
        lock
    );
    let publication = process::record(&cooked, "TWO_LEVEL_PACKAGE_PUBLISHED ");
    assert!(publication["postpublication_error"].is_null());
    assert_eq!(publication["runtime"], "audio-desktop");
    assert_eq!(publication["executable"], "player_audio.exe");
    let published = output.join("TwoLevelAudio").canonicalize().unwrap();
    assert_eq!(
        Path::new(publication["directory"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        published
    );
    package::assert_payload(&published);
    let manifest_bytes = read_bounded(&published.join("package.json"), 1024 * 1024);
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    let payload = &manifest["payload"];
    package::inspect(
        &published,
        payload,
        &triple,
        &configuration,
        &source_samples,
    );
    let bootstrap = payload["generated_input_path"].as_str().unwrap();
    assert_eq!(bootstrap, "src/bin/player/generated.rs");
    assert!(RelativePath::new(bootstrap).is_ok());
    assert_eq!(
        payload["generated_input_digest"],
        package::digest(ContentDigest::of_bytes(&read_bounded(
            &fixture.project.join(bootstrap),
            32 * 1024
        )))
    );
    let inputs = payload["build"]["declared_inputs"].as_array().unwrap();
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/lib.rs",
        "src/sound_bank.rs",
        "src/sound_bank/source.rs",
        "src/sound_bank/runtime.rs",
        "src/sound_bank/wav.rs",
        "src/audio.rs",
        "src/audio/bank.rs",
        "src/bin/player_audio.rs",
        "src/bin/player_audio/arguments.rs",
        "src/bin/player_audio/observation.rs",
        "src/bin/player_audio/oracle.rs",
        "src/bin/player_desktop/runtime.rs",
        bootstrap,
    ] {
        assert!(
            inputs.iter().any(|input| input["path"] == required),
            "missing actual cooked audio build input: {required}"
        );
    }
    let package_digests = ["player_audio.exe", "content.pack", "package.json"].map(|name| {
        host_fs::readable_file(&published.join(name))
            .unwrap()
            .digest(512 * 1024 * 1024)
            .unwrap()
    });
    let relocated = fixture
        .directory("relocated-installation")
        .join("TwoLevelAudio");
    assert!(!relocated.exists() && published.starts_with(&fixture.root));
    fs::rename(&published, &relocated).unwrap();
    assert!(!published.exists());
    let corrupt = fixture.directory("corrupt-installation");
    let mismatched = fixture.directory("mismatched-installation");
    package::copy_package(&relocated, &corrupt);
    package::copy_package(&relocated, &mismatched);
    package::corrupt_bank(&corrupt.join("content.pack"));
    package::mismatch_catalog(&mismatched.join("content.pack"));
    let source = fixture.project.canonicalize().unwrap();
    let cache = output.join("two-level-cook-cache").canonicalize().unwrap();
    for (original, name) in [
        (&source, "retired-copied-source"),
        (&cache, "retired-copied-cache"),
    ] {
        let hidden = fixture.root.join(name);
        assert!(original.starts_with(&fixture.root) && !target.starts_with(original));
        assert!(!fixture.repository.starts_with(original));
        assert!(!hidden.exists() && hidden.parent() == Some(fixture.root.as_path()));
        assert_eq!(original.canonicalize().unwrap(), *original);
        fs::rename(original, &hidden).unwrap();
        assert!(!original.exists() && hidden.is_dir());
        let retired = hidden.canonicalize().unwrap();
        assert_eq!(retired, hidden);
        assert_eq!(retired.parent(), Some(fixture.root.as_path()));
        assert!(!fixture.repository.starts_with(&retired) && !target.starts_with(&retired));
        fs::remove_dir_all(&retired).unwrap();
        assert!(!retired.exists());
    }
    let worker = fixture.directory("sealed-audio-worker");
    let cwd = worker.join("empty");
    fs::create_dir(&cwd).unwrap();
    let request = Request {
        executable: relocated.join("player_audio.exe"),
        corrupt_executable: corrupt.join("player_audio.exe"),
        mismatched_executable: mismatched.join("player_audio.exe"),
        cwd,
        invalid_path: worker.join("no-cargo-or-rustc"),
        local_data: fixture.directory("player-local-data"),
        roaming_data: fixture.directory("player-roaming-data"),
        common_data: fixture.directory("player-common-data"),
        unavailable_source: source,
        unavailable_cache: cache,
        unresolved_cleanup: worker.join("unresolved-cleanup"),
        title,
        target: triple,
        catalog_digest: payload["catalog_digest"].clone(),
        source_samples,
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
    let text = custody.run(NativeProcess::new(owner, None), Duration::from_secs(5 * 60));
    let report = process::record(&text, REPORT);
    assert_eq!(report["success"], true);
    assert_eq!(report["verified_processes"], 1);
    assert_eq!(report["physical_close_processes"], 1);
    assert_eq!(report["refused_processes"], 2);
    println!("{REPORT}{report}");
    package::assert_payload(&relocated);
    for (name, expected) in ["player_audio.exe", "content.pack", "package.json"]
        .into_iter()
        .zip(package_digests)
    {
        assert_eq!(
            host_fs::readable_file(&relocated.join(name))
                .unwrap()
                .digest(512 * 1024 * 1024)
                .unwrap(),
            expected
        );
    }
    assert_eq!(
        read_bounded(&relocated.join("package.json"), 1024 * 1024),
        manifest_bytes
    );
    assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
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
            "unsealed worker environment name: {name:?}"
        );
    }
    let request: Request = serde_json::from_slice(&read_bounded(
        &std::env::current_dir().unwrap().join(REQUEST),
        512 * 1024,
    ))
    .unwrap();
    for path in [
        &request.executable,
        &request.corrupt_executable,
        &request.mismatched_executable,
        &request.cwd,
        &request.invalid_path,
        &request.local_data,
        &request.roaming_data,
        &request.common_data,
        &request.unavailable_source,
        &request.unavailable_cache,
        &request.unresolved_cleanup,
    ] {
        assert!(path.is_absolute());
    }
    assert!(!request.invalid_path.exists());
    assert!(!request.unavailable_source.exists() && !request.unavailable_cache.exists());
    workflow::run(&request);
}
