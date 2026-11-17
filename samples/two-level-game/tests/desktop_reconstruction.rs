//! Windows native sequential reconstruction, supervised outside its main-thread child.

#![cfg(all(feature = "desktop", windows, feature = "development"))]
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
#[ignore = "requires Windows native hardware, a focused desktop and physical A/D/W/S up; run serially with WGPU_BACKEND=dx12 or vulkan and NARA_WGPU_FORCE_FALLBACK=0"]
fn public_desktop_reconstructs_complete_game_on_recovered_loop_and_reconciles_input() {
    let backend = std::env::var("WGPU_BACKEND").expect("select exactly one native backend");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_desktop_reconstruction"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .environment("WGPU_BACKEND", backend)
    .environment("NARA_WGPU_FORCE_FALLBACK", "0");
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(512, 256 * 1024).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut termination_started = None;
    let mut failure = None;
    loop {
        let now = Instant::now();
        if let Err(error) = process.advance(now) {
            failure.get_or_insert_with(|| format!("original child observation failed: {error}"));
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if termination_started.is_none() && (now >= deadline || failure.is_some()) {
            failure.get_or_insert_with(|| {
                "desktop reconstruction exceeded its parent deadline".into()
            });
            if let Err(error) = process.request_terminate(now) {
                failure = Some(format!(
                    "{}; original termination failed: {error}",
                    failure.as_deref().unwrap()
                ));
            }
            termination_started = Some(now);
        }
        if termination_started
            .is_some_and(|start| now.saturating_duration_since(start) >= Duration::from_secs(15))
        {
            let state = process.state();
            std::mem::forget(process);
            panic!("exact child cleanup remains retained at process exit ({state:?}): {failure:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = process
        .logs()
        .iter()
        .map(|entry| String::from_utf8_lossy(entry.bytes()).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let receipt = process
        .cleanup_receipt()
        .expect("the original child must publish physical cleanup");
    assert!(failure.is_none(), "{failure:?}\n{output}");
    assert!(
        receipt.exit().success(),
        "native reconstruction failed: {receipt:?}\n{output}"
    );
    assert!(receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained());
    assert!(!receipt.force_killed());
    assert!(!process.output_loss_observed());
    for expected in [
        "two_level_desktop_reconstruction_source: ok canonical_bytes=",
        "two_level_desktop_reconstruction_preflight: ok deadline_refused=true configuration_refused=true fresh_capture_unchanged=true",
        "two_level_desktop_reconstruction_retired: role=original native_destroyed=true surface_retired=true provider_released=true physical_loop_recovered=true",
        "two_level_desktop_reconstruction_retired: role=candidate native_destroyed=true surface_retired=true provider_released=true physical_loop_recovered=true",
        "two_level_desktop_reconstruction_continuation: ok held_steps=3 new_started_commands=0 release_gap_stop=true no_stuck_hold=true",
        "two_level_desktop_reconstruction_pixels: ok original_app_frame=",
        "two_level_desktop_reconstruction_animation: ok atlas_frames=4 paused_equal=true retained_replays=6 restored_first_frame=true",
        "equal_replays=true",
        "two_level_desktop_reconstruction: ok sequential_runtimes=true shared_physical_loop=true canonical_bytes=true fresh_owners=true restored_presentation=true native_cleanup=true",
    ] {
        assert!(output.contains(expected), "missing {expected:?}:\n{output}");
    }
}
