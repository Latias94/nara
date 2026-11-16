#![cfg(all(
    feature = "desktop",
    any(windows, target_os = "linux"),
    feature = "development"
))]
#![forbid(unsafe_code)]

use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::{
    ProcessLogLimits, SupervisedProcessCommand, SupervisedProcessOwner, SupervisedProcessState,
};

#[test]
#[ignore = "requires native hardware and a desktop; run serially with explicit WGPU_BACKEND and NARA_WGPU_FORCE_FALLBACK=0"]
fn public_desktop_poll_controls_real_gameplay_and_retires_original_native_owners() {
    let backend = std::env::var("WGPU_BACKEND").expect("select exactly one native backend");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_desktop_control"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .environment("WGPU_BACKEND", backend)
    .environment("NARA_WGPU_FORCE_FALLBACK", "0");
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(512, 256 * 1024).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut timed_out = false;
    loop {
        process.advance(Instant::now()).unwrap();
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if !timed_out && Instant::now() >= deadline {
            timed_out = true;
            process.request_terminate(Instant::now()).unwrap();
        }
        if timed_out && Instant::now() >= deadline + Duration::from_secs(15) {
            std::mem::forget(process);
            panic!("the exact native control process still owns incomplete cleanup");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = process
        .logs()
        .iter()
        .map(|entry| String::from_utf8_lossy(entry.bytes()).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let receipt = process.cleanup_receipt().unwrap();
    assert!(!timed_out, "native control timed out: {output}");
    assert!(
        receipt.exit().success(),
        "native game failed: {receipt:?}\n{output}"
    );
    assert!(receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained());
    assert!(!receipt.force_killed());
    assert!(!process.output_loss_observed());
    assert!(output.contains("two_level_desktop_keyboard: ok restored_hold=true release_gap_reconciled=true stopped_before_movement=true"), "{output}");
    assert!(output.contains("two_level_desktop_control: ok paused_capture=true exact_steps=true ordinary_gameplay=true original_native_retired=true"), "{output}");
}
