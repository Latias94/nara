#![cfg(target_os = "windows")]

use std::{
    fs::{self, File},
    os::windows::process::CommandExt,
    path::Path,
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[test]
#[ignore = "requires Windows/DX12 and briefly opens an ordinary visible native Editor window"]
fn external_source_panel_runs_through_the_public_native_runner_and_exits_normally() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let artifacts = std::env::temp_dir().join(format!(
        "nara-external-native-panel-{}-{unique}",
        std::process::id(),
    ));
    fs::create_dir(&artifacts)
        .expect("the native proof must exclusively own its new artifact directory");
    for directory in [
        "cwd",
        "appdata",
        "localappdata",
        "home",
        "config",
        "data",
        "cache",
    ] {
        fs::create_dir(artifacts.join(directory)).unwrap();
    }
    let stdout_path = artifacts.join("stdout.log");
    let stderr_path = artifacts.join("stderr.log");
    let project = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../generic-editor/known-unbound")
        .canonicalize()
        .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_native_panel_canary"));
    command
        .arg(project)
        .current_dir(artifacts.join("cwd"))
        // Windows user_default reads APPDATA. Only this child gets the isolated preferences;
        // no parent environment or real user configuration is changed.
        .env("APPDATA", artifacts.join("appdata"))
        .env("XDG_CONFIG_HOME", artifacts.join("config"))
        .env("XDG_DATA_HOME", artifacts.join("data"))
        .env("XDG_CACHE_HOME", artifacts.join("cache"))
        .env("WGPU_BACKEND", "dx12")
        .env("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER", "1")
        .stdout(File::create(&stdout_path).unwrap())
        .stderr(File::create(&stderr_path).unwrap())
        // Suppress a separate console only. The ordinary public Editor window is not hidden.
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
                    "native runner failed to finish within its owned watchdog: poll={wait:?}; kill={kill:?}; wait={joined:?}",
                ));
            }
        }
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap();
    let stderr = fs::read_to_string(&stderr_path).unwrap();
    let status = result.unwrap_or_else(|error| {
        panic!(
            "{error}\nartifacts: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            artifacts.display(),
        )
    });
    assert!(
        status.success(),
        "native child failed: {status}\nartifacts: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        artifacts.display()
    );
    assert!(
        stdout.contains("NARA_EXTERNAL_NATIVE_PANEL_OK"),
        "normal native runner return did not confirm its panel lifecycle\nartifacts: {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        artifacts.display()
    );
    eprintln!(
        "external native panel proof artifacts: {}",
        artifacts.display()
    );
}
