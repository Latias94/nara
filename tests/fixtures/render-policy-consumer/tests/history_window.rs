#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires selected native hardware, an interactive desktop and the prebuilt history_window example"]
fn normal_runtime_window_advances_four_original_histories_and_retires() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one native backend: dx12, vulkan or metal",
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "native qualification requires NARA_WGPU_FORCE_FALLBACK=0",
    );
    // The example owns a real main thread for the Windows event loop. Never invoke Cargo here.
    let executable = std::env::var_os("NARA_RENDER_POLICY_HISTORY_WINDOW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().expect("locate this test executable");
            let profile = test
                .parent()
                .and_then(Path::parent)
                .expect("the test executable is under the selected profile's deps directory");
            profile
                .join("examples")
                .join(format!("history_window{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_file(),
        "build --example history_window first: {}",
        executable.display(),
    );
    let mut child = Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the actual temporal Runtime/window consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("observe the finite window subprocess")
        {
            assert!(
                status.success(),
                "native temporal window child failed: {status}",
            );
            return;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop the timed-out child");
            child.wait().expect("reap the timed-out child");
            panic!("temporal window child exceeded 90 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
