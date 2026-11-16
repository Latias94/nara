//! Native acceptance starts the ordinary executable on its own Windows main thread.

#![cfg(all(
    windows,
    feature = "development",
    feature = "desktop",
    feature = "audio-cpal"
))]
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
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Report {
    format: String,
    accepted: bool,
    failure: Option<String>,
    runs: Vec<Run>,
    fresh_runtime_generation: bool,
    retired_generation_unchanged: bool,
    physical_event_loop_recovered_and_dropped: bool,
    temporary_project_removed: bool,
}

#[derive(Debug, Deserialize)]
struct Run {
    trajectory: String,
    failure: Option<String>,
    game: Option<Game>,
    controls: Option<Controls>,
    audio: Option<Audio>,
    pcm: Option<Pcm>,
    native: Native,
}

#[derive(Debug, Deserialize)]
struct Game {
    score: u32,
    collected: usize,
    travel_count: u32,
    restarts: u32,
    saves: u32,
    restores: u32,
    script_verified: bool,
}

#[derive(Debug, Deserialize)]
struct Controls {
    exact_steps: u64,
    pickup_step_tick: u64,
    paused_tick: u64,
    paused_output_start: u64,
    paused_output_end: u64,
    resumed_tick: u64,
    failed_travel: Option<Game>,
    failed_travel_error: Option<String>,
    failed_travel_audio_accepted: u64,
    retry_before_instance: Option<u64>,
    retry_after_instance: Option<u64>,
    retry_audio_accepted: u64,
    save_restore_audio_accepted: u64,
}

#[derive(Debug, Deserialize)]
struct Audio {
    runtime: u64,
    accepted: u64,
    started: u64,
    completed: u64,
    cancelled: u64,
    callbacks: u64,
    output_frames: u64,
    faults: u64,
    admission_open: bool,
    stream_drop_returned: bool,
    data_callback_released: bool,
    error_callback_released: bool,
    physical_complete: bool,
    play_admitted: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Pcm {
    frames: u64,
    samples_compared: usize,
    nonzero_samples: usize,
    maximum_absolute_error: f32,
    accepted_pickups: usize,
    accepted_travels: usize,
    stale_scope_cancellations: usize,
    scope_applications: usize,
    validated_scene_instances: usize,
    pickup_source_frames: usize,
    travel_source_frames: usize,
}

#[derive(Debug, Deserialize)]
struct Native {
    runtime_terminal: bool,
    world_unavailable: bool,
    native_destroyed: bool,
    surface_retired: bool,
    provider_released: bool,
    target_fault_absent: bool,
    physical_event_loop_recovered: bool,
    original_presentations: u64,
    adapter: Option<Adapter>,
}

#[derive(Debug, Deserialize)]
struct Adapter {
    backend: String,
    device_type: String,
    backend_instance: u64,
    device_epoch: u64,
}

#[test]
#[ignore = "requires a real WASAPI output device and an interactive hardware Windows desktop; run serially with WGPU_BACKEND=dx12 or vulkan and NARA_WGPU_FORCE_FALLBACK=0"]
fn ordinary_audible_game_matches_original_wavs_and_retires_original_native_owners() {
    let backend = std::env::var("WGPU_BACKEND").expect("select exactly one hardware backend");
    assert!(matches!(backend.as_str(), "dx12" | "vulkan"));
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0")
    );
    let command = SupervisedProcessCommand::new(
        env!("CARGO_BIN_EXE_audio_desktop"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .argument("--verify")
    .environment("WGPU_BACKEND", backend.as_str())
    .environment("NARA_WGPU_FORCE_FALLBACK", "0");
    let mut process = SupervisedProcessOwner::spawn(
        &command,
        ProcessLogLimits::try_from_counts(2048, 1024 * 1024).unwrap(),
    )
    .expect("start the ordinary main-thread audible game");
    let deadline = Instant::now() + Duration::from_secs(240);
    let mut termination_started = None;
    let mut supervision_error = None;
    let mut timed_out = false;
    loop {
        let now = Instant::now();
        if let Err(error) = process.advance(now) {
            supervision_error.get_or_insert_with(|| error.to_string());
        }
        if process.state() == SupervisedProcessState::Complete {
            break;
        }
        if termination_started.is_none() && (now >= deadline || supervision_error.is_some()) {
            timed_out = now >= deadline;
            termination_started = Some(now);
            if let Err(error) = process.request_terminate(now) {
                std::mem::forget(process);
                panic!(
                    "original native audio child retains unresolved process custody after termination failed: {error}"
                );
            }
        }
        if termination_started.is_some_and(|started| now >= started + Duration::from_secs(15)) {
            std::mem::forget(process);
            panic!(
                "original native audio child still retains incomplete containment or output-reader cleanup"
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
    let stdout = process
        .logs()
        .iter()
        .filter(|entry| entry.stream() == ProcessOutputStream::Stdout)
        .flat_map(|entry| entry.bytes().iter().copied())
        .collect::<Vec<_>>();
    let stdout = String::from_utf8(stdout).expect("complete original stdout is UTF-8");
    let diagnostics = process
        .logs()
        .iter()
        .map(|entry| entry.text())
        .collect::<Vec<_>>()
        .join("\n");
    let receipt = process
        .cleanup_receipt()
        .expect("original child physical cleanup receipt");
    assert!(
        !timed_out && supervision_error.is_none(),
        "native audio supervision failed: {supervision_error:?}\n{diagnostics}"
    );
    assert!(
        receipt.exit().success(),
        "native audio prerequisite or gameplay failed: {receipt:?}\n{diagnostics}"
    );
    assert!(
        receipt.containment_empty() && receipt.readers_stopped() && receipt.output_drained(),
        "{receipt:?}"
    );
    assert!(
        !receipt.force_killed() && !process.output_loss_observed(),
        "{receipt:?}\n{diagnostics}"
    );
    let log_stats = process.logs().stats();
    assert_eq!(log_stats.dropped_records(), 0);
    assert_eq!(log_stats.dropped_bytes(), 0);
    assert_eq!(log_stats.truncated_records(), 0);
    let reports = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Report>(line).ok())
        .filter(|report| report.format == "two-level-native-game-audio/v1")
        .collect::<Vec<_>>();
    assert_eq!(
        reports.len(),
        1,
        "missing or duplicate typed native audio report: {diagnostics}"
    );
    let report = &reports[0];
    assert!(report.accepted && report.failure.is_none(), "{report:?}");
    assert!(
        report.fresh_runtime_generation
            && report.retired_generation_unchanged
            && report.physical_event_loop_recovered_and_dropped
            && report.temporary_project_removed,
        "{report:?}"
    );
    assert_eq!(report.runs.len(), 2);
    for (index, run) in report.runs.iter().enumerate() {
        assert!(run.failure.is_none(), "{run:?}");
        let native = &run.native;
        assert!(
            native.runtime_terminal
                && native.world_unavailable
                && native.native_destroyed
                && native.surface_retired
                && native.provider_released
                && native.target_fault_absent
                && native.physical_event_loop_recovered
                && native.original_presentations > 0,
            "{native:?}"
        );
        let adapter = native
            .adapter
            .as_ref()
            .expect("actual original hardware backend");
        assert_eq!(adapter.backend, backend);
        assert!(matches!(
            adapter.device_type.as_str(),
            "discrete-gpu" | "integrated-gpu"
        ));
        assert!(adapter.backend_instance != 0 && adapter.device_epoch != 0);
        let audio = run.audio.as_ref().expect("post-World read witness");
        assert!(audio.runtime != 0 && audio.callbacks > 0 && audio.output_frames > 0);
        assert_eq!(audio.faults, 0);
        assert!(
            audio.play_admitted
                && audio.error.is_none()
                && !audio.admission_open
                && audio.stream_drop_returned
                && audio.data_callback_released
                && audio.error_callback_released
                && audio.physical_complete,
            "{audio:?}"
        );
        assert_eq!(audio.completed + audio.cancelled, audio.accepted);
        assert!(audio.started <= audio.accepted);
        let pcm = run
            .pcm
            .as_ref()
            .expect("independently decoded complete callback comparison");
        assert_eq!(pcm.frames, audio.output_frames);
        assert_eq!(pcm.samples_compared as u64, pcm.frames * 2);
        assert!(pcm.frames <= 48_000 * 60 && pcm.nonzero_samples > 0);
        assert!(pcm.maximum_absolute_error <= 0.000_001);
        assert_eq!(
            (pcm.pickup_source_frames, pcm.travel_source_frames),
            (5760, 11520)
        );
        assert!(pcm.scope_applications >= 3);
        assert!(pcm.validated_scene_instances >= 3);
        let game = run.game.as_ref().expect("actual final gameplay facts");
        if index == 0 {
            assert_eq!(run.trajectory, "interactive_commands");
            assert_eq!(
                (audio.accepted, pcm.accepted_pickups, pcm.accepted_travels),
                (2, 1, 1)
            );
            assert!(pcm.stale_scope_cancellations > 0);
            assert_eq!(
                (
                    game.score,
                    game.collected,
                    game.travel_count,
                    game.restarts,
                    game.saves,
                    game.restores
                ),
                (10, 1, 2, 1, 1, 1)
            );
            let controls = run
                .controls
                .as_ref()
                .expect("actual Pause/Step/Resume and failed travel");
            assert!(controls.exact_steps > 1 && controls.pickup_step_tick > 0);
            assert_eq!(controls.pickup_step_tick, controls.paused_tick);
            assert!(controls.paused_output_end >= controls.paused_output_start + 12_000);
            assert!(controls.resumed_tick > controls.paused_tick);
            let refused = controls
                .failed_travel
                .as_ref()
                .expect("actual failed travel snapshot");
            assert_eq!(
                (
                    refused.score,
                    refused.collected,
                    refused.travel_count,
                    refused.restarts
                ),
                (10, 1, 0, 0)
            );
            assert!(
                controls
                    .failed_travel_error
                    .as_ref()
                    .is_some_and(|error| !error.is_empty())
            );
            assert_eq!(controls.failed_travel_audio_accepted, 1);
            assert!(
                controls.retry_before_instance.is_some() && controls.retry_after_instance.is_some()
            );
            assert_ne!(
                controls.retry_before_instance,
                controls.retry_after_instance
            );
            assert_eq!(controls.retry_audio_accepted, 2);
            assert_eq!(controls.save_restore_audio_accepted, 2);
        } else {
            assert_eq!(run.trajectory, "ordinary_travel_script");
            assert!(game.script_verified && run.controls.is_none());
            assert_eq!(
                (game.score, game.collected, game.travel_count, game.restarts),
                (30, 3, 4, 1)
            );
            assert_eq!(
                (audio.accepted, pcm.accepted_pickups, pcm.accepted_travels),
                (7, 3, 4)
            );
        }
    }
    assert_ne!(
        report.runs[0].audio.as_ref().unwrap().runtime,
        report.runs[1].audio.as_ref().unwrap().runtime
    );
    let marker = "TWO_LEVEL_AUDIO_DESKTOP_OK gameplay=true independent_pcm=true original_native_retired=true";
    assert_eq!(
        stdout.lines().filter(|line| *line == marker).count(),
        1,
        "{diagnostics}"
    );
}
