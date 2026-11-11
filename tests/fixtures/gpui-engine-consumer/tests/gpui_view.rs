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
    let (output, logs) = run_native_gpui("--smoke", Duration::from_secs(105), 128 * 1024);
    for marker in [
        "NARA_GPUI_VIEW_PIXELS",
        "NARA_GPUI_VIEW_WORKFLOW",
        "NARA_GPUI_VIEW_SCALED",
        "NARA_GPUI_VIEW_OK",
        "NARA_GPUI_STATE_LOCALITY",
        "NARA_GPUI_COLLECTION",
        "NARA_GPUI_CONTENT_PIXELS mode=source",
        "NARA_GPUI_CONTENT_PIXELS mode=processed",
    ] {
        assert!(
            output.lines().any(|line| line.starts_with(marker)),
            "missing {marker}; logs: {}\n{output}",
            logs.display()
        );
    }
}

#[cfg(feature = "ui-work-observation")]
#[test]
#[ignore = "requires an actual Windows GPU and gpui_view prebuilt with ui-work-observation"]
fn measure_hosted_gpui_work() {
    let (output, logs) = run_native_gpui("--measure-ui-work", Duration::from_secs(330), 256 * 1024);
    assert!(
        output
            .lines()
            .any(|line| line.starts_with("NARA_GPUI_WORK_CONFIG warmups=8 samples=64 ")),
        "missing native GPUI work configuration; logs: {}\n{output}",
        logs.display()
    );
    for mode in [
        format!(
            "allocation_observer={}",
            cfg!(feature = "ui-work-allocations")
        ),
        format!("debug_assertions={}", cfg!(debug_assertions)),
    ] {
        assert!(
            output.lines().any(|line| {
                line.starts_with("NARA_GPUI_WORK_CONFIG ")
                    && line.split_whitespace().any(|field| field == mode)
            }),
            "prebuild the example with the matching profile/features ({mode}); logs: {}\n{output}",
            logs.display()
        );
    }
    for case in [
        "equal_observation",
        "static_panels_live_preview",
        "local_property_edit",
        "list_height_change",
        "property_edit_broad_refresh",
    ] {
        for marker in [
            format!("NARA_GPUI_WORK_COUNTS case={case} samples=64 "),
            format!("NARA_GPUI_WORK_CPU case={case} phase=query_snapshot samples=64 "),
        ] {
            assert!(
                output.lines().any(|line| line.starts_with(&marker)),
                "missing {marker}; logs: {}\n{output}",
                logs.display()
            );
        }
        if matches!(case, "local_property_edit" | "property_edit_broad_refresh") {
            for phase in [
                "source_selection",
                "source_prepare",
                "source_commit",
                "source_drive",
            ] {
                let marker = format!("NARA_GPUI_WORK_CPU case={case} phase={phase} samples=64 ");
                assert!(
                    output.lines().any(|line| line.starts_with(&marker)),
                    "missing {marker}; logs: {}\n{output}",
                    logs.display()
                );
                #[cfg(feature = "ui-work-allocations")]
                {
                    let marker = format!(
                        "NARA_GPUI_WORK_ALLOC case={case} phase={phase} scope=calling_thread "
                    );
                    assert!(
                        output.lines().any(|line| line.starts_with(&marker)),
                        "missing {marker}; logs: {}\n{output}",
                        logs.display()
                    );
                }
            }
            for phase in [
                "editor_preview",
                "editor_preview_compile",
                "editor_preview_drive",
                "editor_preview_runtime",
                "editor_preview_materialize",
                "editor_publish_model",
            ] {
                let marker = format!(
                    "NARA_GPUI_WORK_SPANS case={case} phase={phase} calls=64 scope=current_thread_nested_in_mutation"
                );
                assert!(
                    output.lines().any(|line| line == marker),
                    "missing {marker}; logs: {}\n{output}",
                    logs.display()
                );
            }
        }
        #[cfg(feature = "ui-work-allocations")]
        {
            for phase in ["mutation", "query_snapshot", "publish", "preview", "settle"] {
                let marker =
                    format!("NARA_GPUI_WORK_ALLOC case={case} phase={phase} scope=calling_thread ");
                assert!(
                    output.lines().any(|line| line.starts_with(&marker)),
                    "missing {marker}; logs: {}\n{output}",
                    logs.display()
                );
            }
            let marker =
                format!("NARA_GPUI_WORK_HEAP case={case} scope=process_requested_not_rss ");
            assert!(
                output.lines().any(|line| line.starts_with(&marker)),
                "missing {marker}; logs: {}\n{output}",
                logs.display()
            );
        }
    }
    assert!(
        output.lines().any(|line| {
            line == "NARA_GPUI_WORK_OK cases=5 retained_native_frames=0 diagnostics=0 original_host=true"
        }),
        "missing native GPUI work retirement marker; logs: {}\n{output}",
        logs.display()
    );
}

fn run_native_gpui(mode: &str, timeout: Duration, max_log_bytes: u64) -> (String, PathBuf) {
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
        .arg(mode)
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + timeout;
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
        assert!(
            fs::metadata(path).unwrap().len() <= max_log_bytes,
            "native GPUI log exceeded {max_log_bytes} bytes: {}",
            path.display()
        );
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
    (output, logs)
}
