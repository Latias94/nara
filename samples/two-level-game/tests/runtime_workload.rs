//! Physical native acceptance for the ordinary active-gameplay measurement executable.

#![cfg(all(
    windows,
    feature = "desktop",
    feature = "cost-measurement",
    feature = "development"
))]
#![forbid(unsafe_code)]

use std::{
    fs,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara::project_tooling::process::{
    ProcessLogLimits, SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
};

#[test]
#[ignore = "three real hardware DX12 gameplay runs; execute serially on an interactive Windows desktop"]
fn ordinary_active_gameplay_records_original_frames_and_physically_retires_every_run() {
    execute(false);
}

#[test]
#[ignore = "requires a hardware DX12 desktop; proves original native cleanup after an injected observer panic"]
fn a_registered_observer_panic_retires_the_original_window_before_failure_returns() {
    execute(true);
}

fn execute(fail_observer: bool) {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "nara-runtime-workload-{}-{stamp}",
        std::process::id(),
    ));
    fs::create_dir(&directory).unwrap();
    let output_path = directory.join("report.json");
    let mut command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_runtime_workload"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .environment("WGPU_BACKEND", "dx12")
    .environment("NARA_WGPU_FORCE_FALLBACK", "0")
    .environment("NARA_RUNTIME_WORKLOAD_OUTPUT", output_path.as_os_str());
    if fail_observer {
        command = command.argument("--fail-observer");
    }
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(2048, 1024 * 1024).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(300);
    let mut timed_out = false;
    loop {
        process.advance(Instant::now()).unwrap();
        if !timed_out && Instant::now() >= deadline {
            timed_out = true;
            if process.state() != SupervisedProcessState::Complete {
                process.request_terminate(Instant::now()).unwrap();
            }
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if timed_out && Instant::now() >= deadline + Duration::from_secs(15) {
            std::mem::forget(process);
            panic!(
                "original workload child still retains containment at {}",
                directory.display()
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    let logs = process
        .logs()
        .iter()
        .map(|entry| String::from_utf8_lossy(entry.bytes()).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let receipt = process.cleanup_receipt().unwrap();
    assert!(
        !timed_out,
        "workload deadline expired; report at {}: {logs}",
        directory.display()
    );
    assert_eq!(
        receipt.exit().success(),
        !fail_observer,
        "unexpected workload exit; report at {}: {receipt:?}\n{logs}",
        directory.display()
    );
    assert!(receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained());
    assert!(!receipt.force_killed() && !process.output_loss_observed());
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&output_path).unwrap()).unwrap();
    assert_eq!(report["format"], "two-level-active-runtime-workload/v1");
    assert_eq!(report["accepted"], !fail_observer);
    assert_eq!(report["injected_observer_failure"], fail_observer);
    if fail_observer {
        assert!(
            logs.contains("injected registered workload observer failure"),
            "{logs}"
        );
        assert_eq!(report["runs"].as_array().unwrap().len(), 1);
        assert!(
            report["runs"][0]["failure"]
                .as_str()
                .unwrap()
                .contains("protected boundary")
        );
        assert_eq!(report["runs"][0]["cleanup"]["native_destroyed"], true);
        assert_eq!(
            report["runs"][0]["cleanup"]["physical_event_loop_recovered"],
            true
        );
    } else {
        assert!(logs.contains("RUNTIME_WORKLOAD_OK runs=3 gameplay_verified=true telemetry_complete=true original_native_retired=true"), "{logs}");
        assert_eq!(report["runs"].as_array().unwrap().len(), 3);
        for run in report["runs"].as_array().unwrap() {
            assert_eq!(run["cleanup"]["normal_exit"], true);
            assert_eq!(run["cleanup"]["telemetry_complete"], true);
            assert_eq!(run["cleanup"]["physical_event_loop_recovered"], true);
            assert_eq!(run["cleanup"]["native_destroyed"], true);
            assert_eq!(run["startup_frozen_after_first_presentation"], true);
            let startup = &run["startup"]["operation_wall_ns"];
            assert_eq!(startup.as_object().unwrap().len(), 5);
            assert!(
                startup["native_polling_to_first_presentation"]
                    .as_u64()
                    .unwrap()
                    >= startup["first_native_poll"].as_u64().unwrap()
            );
            let frames = run["observed_trace"].as_array().unwrap();
            assert_eq!(&run["first_presentation"], frames.first().unwrap());
            assert_eq!(
                &run["first_initialized_presentation"],
                frames
                    .iter()
                    .find(|frame| frame["initialized"] == true)
                    .unwrap()
            );
            let entry = run["entry_to_first_presentation_observation_wall_ns"]
                .as_u64()
                .unwrap();
            if let Some(wall) = frames[0]["app_to_post_host_wall_ns"].as_u64() {
                assert!(entry >= wall);
            }
            assert_eq!(run["runtime_phase_order"].as_array().unwrap().len(), 5);
            for frame in frames {
                let wall = &frame["runtime_wall"];
                if frame["initialized"] != true && wall.is_null() {
                    continue;
                }
                let phases = wall["phase_wall_ns"].as_array().unwrap();
                assert_eq!(phases.len(), 5);
                let total = phases
                    .iter()
                    .try_fold(0_u64, |sum, phase| sum.checked_add(phase.as_u64().unwrap()))
                    .unwrap();
                assert_eq!(wall["measured_phase_wall_ns"].as_u64(), Some(total));
                if let Some(frame_wall) = frame["app_to_post_host_wall_ns"].as_u64() {
                    assert_eq!(
                        total.checked_add(wall["other_app_frame_wall_ns"].as_u64().unwrap()),
                        Some(frame_wall),
                    );
                } else {
                    assert!(wall["other_app_frame_wall_ns"].is_null());
                }
                assert_eq!(frame["issued_native_work"].as_object().unwrap().len(), 11);
            }
        }
        if let Some(evidence) = std::env::var_os("NARA_RUNTIME_WORKLOAD_EVIDENCE") {
            let evidence = std::path::PathBuf::from(evidence);
            assert!(evidence.is_absolute());
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(evidence)
                .unwrap()
                .write_all(&fs::read(&output_path).unwrap())
                .unwrap();
        }
    }
    fs::remove_file(output_path).unwrap();
    fs::remove_dir(directory).unwrap();
}
