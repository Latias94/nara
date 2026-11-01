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
#[ignore = "requires a native desktop and the prebuilt runtime_window example on the platform main thread"]
fn actual_runtime_window_presents_original_stock_ui_and_replays_after_retirement() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicit native backend required");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_STOCK_RUNTIME_WINDOW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().unwrap();
            test.parent()
                .and_then(Path::parent)
                .expect("profile/deps test executable")
                .join("examples")
                .join(format!("runtime_window{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --example runtime_window before selecting this native test: {}",
        executable.display()
    );
    let logs = tempfile::tempdir().unwrap();
    let path = logs.path().join("runtime-window.stdout");
    let mut child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::from(File::create(&path).unwrap()))
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the actual prebuilt platform-main-thread consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe the actual native consumer: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native stock Runtime consumer exceeded ninety seconds");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(fs::metadata(&path).unwrap().len() <= 64 * 1024);
    let output = fs::read_to_string(path).unwrap();
    assert!(
        status.success(),
        "native Runtime child failed: {status}\n{output}"
    );
    let summary = output
        .lines()
        .find(|line| line.starts_with("STOCK_RUNTIME_OK "))
        .expect("actual presentation, original UI completion and replay summary required");
    for fact in [
        "native_presentations=1",
        "ordinary_hud=true",
        "original_ui_completed=true",
        "original_runtime_retired=true",
        "offscreen_replays=1",
        "original_input=true",
        "numeric_original_hud=true",
    ] {
        assert!(
            summary.split_ascii_whitespace().any(|field| field == fact),
            "{fact}: {summary}"
        );
    }
    println!("{summary}");
}
