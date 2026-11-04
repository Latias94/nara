//! Launch the prebuilt example on its real main thread. Input is injected, not OS automation.
#![forbid(unsafe_code)]
#![cfg(not(target_arch = "wasm32"))]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "requires a native hardware GPU, desktop and the prebuilt ui_surface example"]
fn independent_surface_presents_routes_input_and_rejects_retired_work() {
    run_native_surface(None);
}

#[test]
#[ignore = "requires a native hardware GPU, desktop and the prebuilt ui_surface example"]
fn independent_surface_settles_pending_work_after_error_and_panic() {
    for mode in ["error", "panic", "mesh-error", "mesh-panic"] {
        run_native_surface(Some(mode));
    }
}

fn run_native_surface(failure_mode: Option<&str>) {
    let backend = std::env::var("WGPU_BACKEND").expect("select WGPU_BACKEND explicitly");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan" | "metal"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let executable = std::env::var_os("NARA_UI_SURFACE_EXE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .and_then(Path::parent)
                .unwrap()
                .join("examples")
                .join(format!("ui_surface{}", std::env::consts::EXE_SUFFIX))
        });
    assert!(
        executable.is_absolute() && executable.is_file(),
        "prebuild --example ui_surface: {}",
        executable.display()
    );
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let logs =
        std::env::temp_dir().join(format!("nara-ui-surface-{}-{unique}", std::process::id()));
    fs::create_dir(&logs).unwrap();
    let stdout = logs.join("stdout.log");
    let stderr = logs.join("stderr.log");
    let mut command = Command::new(executable);
    command
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap());
    if let Some(mode) = failure_mode {
        command.env("NARA_UI_SURFACE_FAILURE_AFTER_SUBMIT", mode);
    } else {
        command.env_remove("NARA_UI_SURFACE_FAILURE_AFTER_SUBMIT");
    }
    let mut child = command
        .spawn()
        .expect("launch the prebuilt native example; never launch Cargo here");
    let deadline = Instant::now() + Duration::from_secs(90);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "native child failed or exceeded ninety seconds: {result:?}; logs: {}",
                    logs.display()
                );
            }
        }
    };
    for path in [&stdout, &stderr] {
        assert!(fs::metadata(path).unwrap().len() <= 64 * 1024);
    }
    let output = fs::read_to_string(stdout).unwrap();
    let errors = fs::read_to_string(stderr).unwrap();
    assert!(
        status.success(),
        "native conformance failed: {status}; logs: {}\n{output}\n{errors}",
        logs.display()
    );
    let expected = if let Some(mode) = failure_mode {
        format!(
            "UI_SURFACE_FAILURE_OK mode={mode} original_permits_settled=true presenter_quiescent=true original_host_retired=true"
        )
    } else {
        "UI_SURFACE_OK native_presentations=19 journal_commits=18 popup_actions=1 underlay_blocked=true stale_input_rejected=true late_acceptance_discarded=true reconstructed=true original_hosts_retired=true keyed_component_presented=true keyed_reorder_preserved=true filtered_preedit_discarded=true keyed_binding_reset=true compatible_compose=true external_curve_presented=true curve_pointer_keyboard=true curve_pan_preserved=true curve_drag_across_presentations=true curve_wheel_units_anchored=true curve_retired=true captured_image_presented=true image_qualification=packet_resource_and_actual_presentation input_delivery=injected_pointer_ime_and_logical_keys".into()
    };
    assert!(
        output.lines().any(|line| line == expected),
        "missing completed native conformance result; logs: {}\n{output}\n{errors}",
        logs.display()
    );
}
