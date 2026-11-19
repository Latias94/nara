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

#[test]
#[ignore = "requires an interactive Windows desktop and DX12 hardware; run serially with WGPU_BACKEND=dx12 and NARA_WGPU_FORCE_FALLBACK=0"]
fn physical_play_flow_presents_ui_and_retires_the_original_native_run() {
    assert_eq!(std::env::var("WGPU_BACKEND").as_deref(), Ok("dx12"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_play_flow_check"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
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
            failure.get_or_insert_with(|| format!("native process observation failed: {error}"));
            if let Err(error) = process.retry_cleanup(Instant::now()) {
                failure
                    .get_or_insert_with(|| format!("native process cleanup retry failed: {error}"));
            }
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if Instant::now() >= deadline && !timed_out {
            timed_out = true;
            failure
                .get_or_insert_with(|| "native play-flow process exceeded 120 seconds".to_owned());
            if let Err(error) = process.request_terminate(Instant::now()) {
                eprintln!("play_flow_desktop deadline cleanup: {error}");
            }
        }
        if Instant::now() >= deadline + Duration::from_secs(15) {
            std::mem::forget(process);
            panic!("original play-flow process retains incomplete physical cleanup: {failure:?}");
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
        .expect("original completed process cleanup receipt");
    assert!(failure.is_none(), "{failure:?}\n{output}");
    assert!(
        receipt.exit().success(),
        "native play flow failed: {receipt:?}\n{output}"
    );
    assert!(
        receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained(),
        "{receipt:?}\n{output}"
    );
    assert!(
        !receipt.termination_requested() && !receipt.force_killed(),
        "native termination cannot qualify play flow: {receipt:?}\n{output}"
    );
    assert!(
        !process.output_loss_observed(),
        "native play-flow output lost evidence: {output}"
    );
    let mut observed: &str = stdout.as_ref();
    for checkpoint in [
        "title",
        "new-round-1",
        "walk-held-d",
        "paused-held-d",
        "paused-released-d",
        "continued-idle",
        "fresh-d",
        "scored-round",
        "traveled-round",
        "returned-title",
        "new-round-2",
        "exited-cleanly",
    ] {
        let marker = format!("two_level_play_flow_checkpoint: {checkpoint} ");
        let position = observed.find(&marker).unwrap_or_else(|| {
            panic!("missing ordered physical checkpoint {checkpoint}: {output}")
        });
        observed = &observed[position + marker.len()..];
    }
    assert!(
        observed.contains("two_level_play_flow: ok physical_input=true presented_ui=true paused_clock=true released_hold=true new_round=true original_native_retired=true"),
        "missing original native acceptance marker: {output}",
    );
}
