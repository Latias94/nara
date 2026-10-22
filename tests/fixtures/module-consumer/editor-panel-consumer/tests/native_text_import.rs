#![cfg(target_os = "windows")]

#[path = "support/text_project.rs"]
mod text_project;

use std::{
    fs,
    os::windows::process::CommandExt,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires Windows/DX12 and briefly opens the public native Editor with a compiled text-import panel"]
fn the_native_editor_imports_and_reimports_an_external_source_domain() {
    let project = text_project::TextProject::new();
    let artifacts = project.root.join("native-evidence");
    fs::create_dir(&artifacts).unwrap();
    for name in ["appdata", "config", "data", "cache", "cwd"] {
        fs::create_dir(artifacts.join(name)).unwrap();
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_native_text_import_canary"));
    command
        .arg(&project.root)
        .current_dir(artifacts.join("cwd"))
        .env("APPDATA", artifacts.join("appdata"))
        .env("XDG_CONFIG_HOME", artifacts.join("config"))
        .env("XDG_DATA_HOME", artifacts.join("data"))
        .env("XDG_CACHE_HOME", artifacts.join("cache"))
        .env("WGPU_BACKEND", "dx12")
        .env("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER", "1")
        .stdout(fs::File::create(artifacts.join("stdout.log")).unwrap())
        .stderr(fs::File::create(artifacts.join("stderr.log")).unwrap())
        .creation_flags(0x0800_0000);
    let mut child = command.spawn().unwrap();
    drop(command);
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "native import watchdog expired; stdout: {}; stderr: {}",
                fs::read_to_string(artifacts.join("stdout.log")).unwrap(),
                fs::read_to_string(artifacts.join("stderr.log")).unwrap()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = fs::read_to_string(artifacts.join("stdout.log")).unwrap();
    let stderr = fs::read_to_string(artifacts.join("stderr.log")).unwrap();
    assert!(
        status.success(),
        "{status}; stdout: {stdout}; stderr: {stderr}"
    );
    assert!(
        stdout.contains("NARA_NATIVE_TEXT_IMPORT_OK"),
        "{stdout}; {stderr}"
    );
    let metadata = fs::read(project.root.join("assets/caption.txt.meta")).unwrap();
    let candidate = nara::asset::AssetMetaIdentityCandidate::decode_json_bytes(&metadata).unwrap();
    assert_eq!(
        candidate
            .admit::<nara_asset_domain_consumer::TextSettings>()
            .unwrap()
            .settings()
            .max_characters,
        128
    );
    project.assert_check_and_play();
}
