#![cfg(target_os = "windows")]

use std::{
    fs::{self, File},
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[test]
#[ignore = "requires Windows/DX12 for hidden-window public embedded Host startup, painting and normal close"]
fn public_embedded_host_paints_in_the_callers_hidden_window_and_closes_normally() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = std::env::temp_dir().join(format!(
        "nara-embedded-native-host-{}-{unique}",
        std::process::id(),
    ));
    fs::create_dir(&artifacts)
        .expect("the embedded native proof must exclusively own its artifact directory");
    for directory in [
        "cwd",
        "appdata",
        "localappdata",
        "home",
        "config",
        "data",
        "cache",
        "state",
    ] {
        fs::create_dir(artifacts.join(directory)).unwrap();
    }
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../generic-editor/known-unbound")
        .canonicalize()
        .unwrap();
    let source_before = [
        "Cargo.toml",
        "nara.toml",
        "src/lib.rs",
        "scripts/tool.rs",
        "catalogs/authoring.bundle.json",
        "worlds/entry.scene.json",
        "worlds/external.scene.json",
        "worlds/notes.txt",
        "blueprints/external.prefab.json",
        "media/icon.png",
        "media/icon.png.meta",
    ]
    .map(|relative| {
        let path = project.join(relative);
        let bytes = fs::read(&path).unwrap();
        (path, bytes)
    });
    let stdout_path = artifacts.join("stdout.log");
    let stderr_path = artifacts.join("stderr.log");
    let mut command = Command::new(env!("CARGO_BIN_EXE_embedded_host_canary"));
    command
        .arg(&project)
        .current_dir(artifacts.join("cwd"))
        .env("APPDATA", artifacts.join("appdata"))
        .env("LOCALAPPDATA", artifacts.join("localappdata"))
        .env("HOME", artifacts.join("home"))
        .env("USERPROFILE", artifacts.join("home"))
        .env("XDG_CONFIG_HOME", artifacts.join("config"))
        .env("XDG_DATA_HOME", artifacts.join("data"))
        .env("XDG_CACHE_HOME", artifacts.join("cache"))
        .env("XDG_STATE_HOME", artifacts.join("state"))
        .env("WGPU_BACKEND", "dx12")
        .env("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER", "1")
        .stdin(Stdio::null())
        .stdout(File::create(&stdout_path).unwrap())
        .stderr(File::create(&stderr_path).unwrap())
        .creation_flags(CREATE_NO_WINDOW);
    let mut child = command.spawn().unwrap();
    drop(command);
    let deadline = Instant::now() + Duration::from_secs(60);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            wait => {
                let kill = child.kill();
                let joined = child.wait();
                break Err(format!(
                    "embedded native Host watchdog failed: poll={wait:?}; kill={kill:?}; wait={joined:?}",
                ));
            }
        }
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    let diagnostics = format!(
        "artifacts: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        artifacts.display(),
    );
    let status = result.unwrap_or_else(|error| panic!("{error}\n{diagnostics}"));
    assert!(
        status.success(),
        "embedded native child failed: {status}\n{diagnostics}",
    );
    assert!(
        stdout
            .lines()
            .any(|line| line.starts_with("NARA_EMBEDDED_NATIVE_HOST_OK ")),
        "the public embedded Host did not confirm clean finish\n{diagnostics}",
    );
    for failure in [
        "panicked at",
        "Error:",
        "Nara Editor GPU recovery paused:",
        "Nara Editor GPU retirement:",
        "Nara Editor Host cleanup failed:",
        "Nara Editor cannot release unfinished native custody",
    ] {
        assert!(
            !stderr.contains(failure),
            "embedded native child reported {failure}\n{diagnostics}",
        );
    }
    for (path, before) in source_before {
        assert_eq!(
            fs::read(&path).unwrap(),
            before,
            "embedded startup/render/close modified fixture source: {}\n{diagnostics}",
            path.display(),
        );
    }
    eprintln!(
        "embedded native Host smoke artifacts: {}",
        artifacts.display()
    );
}
