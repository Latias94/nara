#![cfg(all(windows, feature = "play-flow-check"))]
#![forbid(unsafe_code)]

use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_tooling::process::{
    ProcessLogLimits, ProcessOutputStream, SupervisedProcessCommand, SupervisedProcessOwner,
    SupervisedProcessState,
};
use serde_json::Value;

#[test]
#[ignore = "requires an interactive Windows desktop and DX12 hardware; run serially with WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0"]
fn production_ready_accepts_playing_after_delayed_resume_confirmation() {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_play_flow_check"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .argument("--readiness-after-start")
    .environment("WGPU_BACKEND", "dx12")
    .environment("NARA_WGPU_FORCE_FALLBACK", "0");
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(512, 512 * 1024).unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut failure = None;
    let mut timed_out = false;
    loop {
        if let Err(error) = process.advance(Instant::now()) {
            failure.get_or_insert_with(|| format!("readiness process observation failed: {error}"));
            if let Err(error) = process.retry_cleanup(Instant::now()) {
                eprintln!("readiness process cleanup retry: {error}");
            }
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if Instant::now() >= deadline && !timed_out {
            timed_out = true;
            failure.get_or_insert_with(|| "readiness process exceeded 120 seconds".to_owned());
            if let Err(error) = process.request_terminate(Instant::now()) {
                eprintln!("readiness deadline cleanup: {error}");
            }
        }
        if Instant::now() >= deadline + Duration::from_secs(15) {
            std::mem::forget(process);
            panic!("original readiness process retains incomplete cleanup: {failure:?}");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = process
        .logs()
        .iter()
        .map(|entry| String::from_utf8_lossy(entry.bytes()).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let stdout: Vec<u8> = process
        .logs()
        .iter()
        .filter(|entry| entry.stream() == ProcessOutputStream::Stdout)
        .flat_map(|entry| entry.bytes())
        .copied()
        .collect();
    let stdout = String::from_utf8_lossy(&stdout);
    let receipt = process
        .cleanup_receipt()
        .expect("original readiness cleanup receipt");
    assert!(
        failure.is_none() && receipt.exit().success(),
        "{failure:?}\n{receipt:?}\n{output}"
    );
    assert!(
        receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained(),
        "{receipt:?}\n{output}"
    );
    assert!(
        !receipt.termination_requested()
            && !receipt.force_killed()
            && !process.output_loss_observed(),
        "{receipt:?}\n{output}"
    );
    let mut remaining: &str = stdout.as_ref();
    for marker in [
        "two_level_play_flow_readiness_checkpoint: initial-title ",
        "two_level_play_flow_readiness_checkpoint: playing-before-confirmation ",
        "TWO_LEVEL_DESKTOP_READY ",
        "two_level_play_flow_readiness_checkpoint: ready-after-start ",
        "two_level_play_flow_readiness_checkpoint: exited-cleanly ",
        "TWO_LEVEL_DESKTOP_RESULT ",
        "two_level_play_flow_readiness: ok physical_input=true ready_after_start=true launch_consumed=true original_native_retired=true",
    ] {
        let index = remaining
            .find(marker)
            .unwrap_or_else(|| panic!("missing ordered marker {marker}: {output}"));
        remaining = &remaining[index + marker.len()..];
    }
    let record = |prefix: &str| -> Value {
        let records: Vec<_> = stdout
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .collect();
        assert_eq!(records.len(), 1, "expected exactly one {prefix}: {output}");
        serde_json::from_str(records[0]).unwrap()
    };
    let ready = record("TWO_LEVEL_DESKTOP_READY ");
    let result = record("TWO_LEVEL_DESKTOP_RESULT ");
    for record in [&ready, &result] {
        for field in [
            "success",
            "processed_runtime",
            "launch_admitted",
            "launch_accepted",
            "resume_applied",
            "ready",
            "first_present_verified",
        ] {
            assert_eq!(record[field], true, "{field}: {output}");
        }
        assert_eq!(record["flow"]["phase"], "Playing", "{output}");
        assert_eq!(record["flow"]["round"], 1, "{output}");
        assert_eq!(
            record["flow"]["presented_ui"]["matches_snapshot"], true,
            "{output}"
        );
        assert_eq!(
            record["initial_title_presentation"]["phase"], "Title",
            "{output}"
        );
        assert_eq!(record["initial_title_presentation"]["round"], 0, "{output}");
        assert_eq!(record["adapter"]["backend"], "dx12", "{output}");
        assert!(
            matches!(
                record["adapter"]["device_type"].as_str(),
                Some("discrete-gpu" | "integrated-gpu")
            ),
            "{output}"
        );
        assert!(record["state"]["tick"].as_u64().unwrap() > 0, "{output}");
        let initial = record["initial_title_presentation"]["presented_ui"]["receipt"]["app_frame"]
            .as_u64()
            .unwrap();
        assert!(
            initial < record["presentation"]["app_frame"].as_u64().unwrap(),
            "{output}"
        );
        assert!(
            record["flow"]["presented_ui"]["receipt"]["submitted_glyph_quads"]
                .as_u64()
                .unwrap()
                > 0,
            "{output}"
        );
    }
    assert_eq!(
        ready["initial_title_presentation"], result["initial_title_presentation"],
        "{output}"
    );
    for field in [
        "original_runtime_retired",
        "original_pump_returned",
        "cleanup_complete",
    ] {
        assert_eq!(result[field], true, "{field}: {output}");
    }
}
