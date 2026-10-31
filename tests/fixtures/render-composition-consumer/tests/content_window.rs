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
#[ignore = "requires native hardware, an interactive desktop and the prebuilt content_window example"]
fn source_and_processed_stock_inputs_survive_real_native_retirement_and_backing_removal() {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one native backend: dx12, vulkan or metal"
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "native stock capture requires NARA_WGPU_FORCE_FALLBACK=0"
    );
    let executable = std::env::var_os("NARA_RENDER_COMPOSITION_CONTENT_WINDOW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test = std::env::current_exe().expect("locate this test executable");
            let profile = test
                .parent()
                .and_then(Path::parent)
                .expect("the test executable is under the selected profile's deps directory");
            profile
                .join("examples")
                .join(format!("content_window{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute(),
        "the prebuilt example must have an absolute path"
    );
    assert!(
        executable.is_file(),
        "prebuild --example content_window before this test: {}",
        executable.display()
    );
    let logs = tempfile::tempdir().expect("allocate the native result directory");
    let output_path = logs.path().join("content-window.stdout");
    let output = File::create(&output_path).expect("create the native result log");
    // Only the executable owns the platform main thread. This test never creates an EventLoop or
    // invokes Cargo. A file also prevents verbose driver output from blocking the finite wait.
    let mut child = Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output))
        .stderr(Stdio::inherit())
        .spawn()
        .expect("launch the prebuilt native Source/Processed stock consumer");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("observe the native stock capture child: {error}");
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the native stock capture child exceeded ninety seconds");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(
        fs::metadata(&output_path).unwrap().len() <= 64 * 1024,
        "the finite example unexpectedly produced more than 64 KiB of stdout"
    );
    let output = fs::read_to_string(output_path).expect("read the native result log");
    assert!(
        status.success(),
        "native Source/Processed stock child failed: {status}\n{output}"
    );
    let summary = output
        .lines()
        .find(|line| line.starts_with("CONTENT_WINDOW_OK "))
        .expect(
            "success requires the actual stock capture/presentation/readback/retirement summary",
        );
    let field = |name: &str| {
        summary
            .split_ascii_whitespace()
            .filter_map(|field| field.split_once('='))
            .find_map(|(key, value)| (key == name).then_some(value))
            .unwrap_or_else(|| panic!("the native result omitted {name}: {summary}"))
    };
    for (name, expected) in [
        ("captures", "4"),
        ("runtimes", "2"),
        ("native_presentations", "4"),
        ("original_host_outputs", "4"),
        ("replacement_host_outputs", "4"),
        ("foreign_producer_refusals", "2"),
        ("candidate_owner_refusals", "2"),
    ] {
        assert_eq!(field(name), expected);
    }
    for name in [
        "source_runtime",
        "processed_runtime",
        "original_host",
        "replacement_host",
        "pixels_checked",
    ] {
        assert!(
            field(name).parse::<u64>().unwrap() > 0,
            "missing real native evidence: {name}"
        );
    }
    assert_ne!(field("source_runtime"), field("processed_runtime"));
    assert_ne!(field("original_host"), field("replacement_host"));
    for name in [
        "pump_reused",
        "failed_startup_pump_recovered",
        "original_runtimes_retired",
        "live_stores_emptied",
        "backing_destroyed",
        "source_processed_provenance",
        "retained_originals",
        "hosts_retired",
    ] {
        assert_eq!(
            field(name),
            "true",
            "the native stock workflow did not complete: {name}"
        );
    }
    // This finite Sprite/Image workflow does not claim complete UI, R8, or device-loss coverage.
    println!("{summary}");
}
