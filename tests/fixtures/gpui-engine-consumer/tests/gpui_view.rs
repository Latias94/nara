#![cfg(windows)]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires an actual Windows GPU and the prebuilt gpui_view example"]
fn engine_owned_gpui_renders_edits_picks_and_retires_under_original_host() {
    assert!(matches!(
        std::env::var("WGPU_BACKEND").as_deref(),
        Ok("dx12" | "vulkan")
    ));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_GPUI_VIEW_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .and_then(Path::parent)
                .unwrap()
                .join("examples")
                .join(format!("gpui_view{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --example gpui_view"
    );
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let logs = std::env::temp_dir().join(format!("nara-gpui-view-{}-{unique}", std::process::id()));
    fs::create_dir(&logs).unwrap();
    let stdout = logs.join("stdout.log");
    let stderr = logs.join("stderr.log");
    let mut child = Command::new(executable)
        .arg("--smoke")
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(105);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "native GPUI child did not terminate: {result:?}; logs: {}",
                    logs.display()
                );
            }
        }
    };
    for path in [&stdout, &stderr] {
        assert!(fs::metadata(path).unwrap().len() <= 128 * 1024);
    }
    let output = fs::read_to_string(stdout).unwrap();
    let errors = fs::read_to_string(stderr).unwrap();
    assert!(
        !errors.lines().any(|line| line.starts_with("ERROR ")),
        "native/backend validation reported errors; logs: {}\n{errors}",
        logs.display()
    );
    assert!(
        status.success(),
        "native GPUI consumer failed {status}; logs: {}\n{output}\n{errors}",
        logs.display()
    );
    for marker in [
        "NARA_GPUI_VIEW_PIXELS",
        "NARA_GPUI_VIEW_WORKFLOW",
        "NARA_GPUI_VIEW_SCALED",
        "NARA_GPUI_VIEW_OK",
    ] {
        assert!(
            output.lines().any(|line| line.starts_with(marker)),
            "missing {marker}; logs: {}\n{output}",
            logs.display()
        );
    }
}
