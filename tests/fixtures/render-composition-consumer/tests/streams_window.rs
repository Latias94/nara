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
#[ignore = "requires native hardware, an interactive desktop and the prebuilt streams_window example"]
fn two_native_windows_and_an_independent_image_keep_exact_outputs_and_retire() {
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
    let executable = std::env::var_os("NARA_RENDER_COMPOSITION_STREAMS_WINDOW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().expect("locate this test executable");
            let profile = test
                .parent()
                .and_then(Path::parent)
                .expect("the test executable is under the selected profile's deps directory");
            profile
                .join("examples")
                .join(format!("streams_window{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute(),
        "the prebuilt example must have an absolute path"
    );
    assert!(
        executable.is_file(),
        "prebuild --example streams_window before this test: {}",
        executable.display(),
    );
    // Redirect to a file so a noisy driver cannot fill a pipe and deadlock the bounded wait.
    let logs = tempfile::tempdir().expect("allocate the native result directory");
    let output_path = logs.path().join("streams-window.stdout");
    let output = File::create(&output_path).expect("create the native result log");
    // The example owns the Windows/macOS main thread. Never invoke Cargo as a child here.
    let mut child = Command::new(&executable)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the prebuilt native two-window/image consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe the native window child: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the native window child exceeded ninety seconds");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(
        fs::metadata(&output_path).unwrap().len() <= 64 * 1024,
        "the finite example unexpectedly produced more than 64 KiB of stdout",
    );
    let output = fs::read_to_string(output_path).expect("read the native result log");
    assert!(
        status.success(),
        "native two-window/image child failed: {status}\n{output}"
    );
    let summary = output
        .lines()
        .find(|line| line.starts_with("STREAMS_WINDOW_OK "))
        .expect("success requires the actual receipt/pixel/retirement summary");
    let field = |name: &str| {
        summary
            .split_ascii_whitespace()
            .filter_map(|field| field.split_once('='))
            .find_map(|(key, value)| (key == name).then_some(value))
            .unwrap_or_else(|| panic!("the native result omitted {name}: {summary}"))
    };
    assert_eq!(field("ticks"), "720");
    assert_eq!(field("schedules"), "30/60/144");
    assert_eq!(field("a_captures"), "2");
    assert_eq!(field("b_captures"), "60");
    assert_eq!(field("image_captures"), "144");
    assert_eq!(field("pixels_checked"), "3072");
    for name in [
        "window_a_receipts",
        "window_b_receipts",
        "old_a_after_close",
        "b_after_close",
        "image_after_close",
        "passive_mirror_receipts",
        "stale_placement_rejections",
        "pixels_checked",
    ] {
        assert!(
            field(name).parse::<u64>().unwrap() > 0,
            "missing native evidence: {name}"
        );
    }
    for name in [
        "frozen_prefix_complete",
        "paint_refused",
        "paint_panicked",
        "pre_present_panicked",
        "a_recovered",
        "host_retired",
        "windows_dropped",
    ] {
        assert_eq!(
            field(name),
            "true",
            "the native lifecycle did not complete: {name}"
        );
    }
    // Native window managers may ignore minimize/size requests. These are observations, not gates.
    for name in ["minimize_observed", "zero_size_observed", "resize_observed"] {
        assert!(matches!(field(name), "true" | "false"));
    }
    println!("{summary}");
}
