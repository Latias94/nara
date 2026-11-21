#![forbid(unsafe_code)]
#![cfg(windows)]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires the prebuilt hud_only_window example and a native Windows desktop with DX12 hardware"]
fn stock_hud_presents_and_activates_while_the_original_camera_is_fully_clipped() {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_STOCK_HUD_ONLY_WINDOW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().unwrap();
            test.parent()
                .and_then(Path::parent)
                .expect("profile/deps test executable")
                .join("examples/hud_only_window.exe")
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --example hud_only_window or select its absolute path with NARA_STOCK_HUD_ONLY_WINDOW_EXE: {}",
        executable.display()
    );
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("hud-only-window.stdout");
    let mut child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(&path).unwrap()))
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the actual prebuilt platform-main-thread consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => break Err(format!("observe the native HUD child: {error}")),
        }
        if Instant::now() >= deadline {
            break Err("the native HUD child exceeded 90 seconds".to_owned());
        }
        thread::sleep(Duration::from_millis(10));
    };
    if status.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    assert!(fs::metadata(&path).unwrap().len() <= 64 * 1024);
    let output = fs::read_to_string(path).unwrap();
    let status = status.unwrap_or_else(|error| panic!("{error}\n{output}"));
    assert!(
        status.success(),
        "native HUD child failed: {status}\n{output}"
    );
    let summary = "HUD_ONLY_WINDOW_OK verified_presentations=4 same_view_recovered=true original_ui_receipt=true menu_activated=true exact_replay_pixels=true native_retired=true";
    assert_eq!(
        output.lines().filter(|line| *line == summary).count(),
        1,
        "{output}"
    );
    print!("{output}");
}
