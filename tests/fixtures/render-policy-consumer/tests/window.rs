#![cfg(not(target_arch = "wasm32"))]

use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires native hardware, an interactive desktop and the prebuilt heatmap_window example"]
fn normal_runtime_window_presents_and_reconstructs_the_original_policy_capture() {
    // Cargo's example has a real main thread; a Rust test worker cannot own a Windows event loop.
    let executable = std::env::var_os("NARA_RENDER_POLICY_WINDOW_EXE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().expect("locate this test executable");
            let profile = test
                .parent()
                .and_then(std::path::Path::parent)
                .expect("the test executable is under the selected profile's deps directory");
            profile
                .join("examples")
                .join(format!("heatmap_window{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_file(),
        "build --example heatmap_window first: {}",
        executable.display(),
    );
    let mut child = Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the actual Runtime/window consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(status) = child
            .try_wait()
            .expect("observe the finite window subprocess")
        {
            assert!(
                status.success(),
                "native presentation/reconstruction child failed: {status}"
            );
            return;
        }
        if Instant::now() >= deadline {
            child.kill().expect("stop the timed-out child");
            child.wait().expect("reap the timed-out child");
            panic!("window child exceeded 90 seconds");
        }
        thread::sleep(Duration::from_millis(10));
    }
}
