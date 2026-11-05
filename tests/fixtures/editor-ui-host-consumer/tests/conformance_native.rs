//! Runs the prebuilt user-owned frontend on its original native main thread.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires native desktop, hardware GPU and the prebuilt conformance_surface example"]
fn public_image_host_accepts_a_user_frontend_without_a_nara_widget_tree() {
    let backend = std::env::var("WGPU_BACKEND").expect("select WGPU_BACKEND explicitly");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_EDITOR_CONFORMANCE_SURFACE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .and_then(Path::parent)
                .unwrap()
                .join("examples")
                .join(format!(
                    "conformance_surface{}",
                    std::env::consts::EXE_SUFFIX
                ))
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --features native --example conformance_surface: {}",
        executable.display()
    );
    let logs = tempfile::Builder::new()
        .prefix("nara-editor-conformance-surface-")
        .tempdir()
        .unwrap();
    let stdout = logs.path().join("stdout.log");
    let stderr = logs.path().join("stderr.log");
    let mut child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                let path = logs.keep();
                panic!(
                    "native conformance process failed or timed out: {result:?}; logs: {}",
                    path.display()
                );
            }
        }
    };
    for path in [&stdout, &stderr] {
        assert!(fs::metadata(path).unwrap().len() <= 64 * 1024);
    }
    let output = fs::read_to_string(&stdout).unwrap();
    let errors = fs::read_to_string(&stderr).unwrap();
    if !status.success() {
        let path = logs.keep();
        panic!(
            "native conformance failed: {status}; logs: {}\n{output}\n{errors}",
            path.display()
        );
    }
    let marker = "EDITOR_FOREIGN_UI_OK native_presentations=18 exact_panel_results=7 readback_pixels=144000 public_image_host=true widgets_required=false source_preserved=true clipboard_cursor_refresh=true capture_modal_retirement=true keyboard_only_focus=true semantic_owner_fenced=true two_panel_atomic=true input_delivery=injected";
    assert!(
        output.lines().any(|line| line == marker),
        "native conformance did not finish its complete trajectory:\n{output}\n{errors}"
    );
}
