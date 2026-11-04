//! Launch the prebuilt native consumer on its original main thread; never launch Cargo here.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires native desktop, hardware GPU and the prebuilt panel_surface example"]
fn independent_panel_host_presents_edits_persists_and_retires_original_work() {
    let backend = std::env::var("WGPU_BACKEND").expect("select WGPU_BACKEND explicitly");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_EDITOR_PANEL_SURFACE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .and_then(Path::parent)
                .unwrap()
                .join("examples")
                .join(format!("panel_surface{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --features native --example panel_surface: {}",
        executable.display()
    );
    let logs = tempfile::Builder::new()
        .prefix("nara-editor-panel-surface-")
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
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                let path = logs.keep();
                panic!(
                    "native panel process failed or timed out: {result:?}; logs: {}",
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
            "native panel failed: {status}; logs: {}\n{output}\n{errors}",
            path.display()
        );
    }
    let marker = "EDITOR_UI_HOST_OK native_presentations=6 journal_commits=5 exact_panel_results=7 nonactive_authoring=true undo_redo=true save_reopen=true source_preserved=true modal_underlay_blocked=true hidden_roundtrip_retired=true late_presentation_discarded=true stale_input_rejected=true original_hosts_retired=true input_delivery=injected_pointer_and_ime";
    assert!(
        output.lines().any(|line| line == marker),
        "native qualification did not reach its final marker:\n{output}\n{errors}"
    );
}
