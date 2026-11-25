use nara::identity::SceneEntityId;
use serde_json::json;

use super::*;
use crate::{
    levels::{LevelId, LevelKey},
    resume::{
        GameClock, LevelContentStamp, PlayerState, ResumeCompatibility, RuntimeCatchUpPolicy,
        RuntimeTimeState, SessionCounters,
    },
    save::{Checkpoint, CheckpointLocation},
};

fn diagnostics(inbox_checks: u64) -> ControlServiceObservation {
    ControlServiceObservation {
        inbox_checks,
        idle_wait_calls: 0,
        idle_wait_nanos: 0,
        hints: 0,
        callbacks: 0,
        coalesced_hints: 0,
        native_wake_delivery_failures: 0,
        watcher_started: true,
        watcher_closed: false,
        counters_saturated: false,
    }
}

fn response(result: Result<Reply, String>) -> Response {
    Response {
        version: PROTOCOL_VERSION,
        placement: DevelopmentPlacement::SourceDesktop,
        session: "session".into(),
        generation: "generation".into(),
        request: 1,
        control_service: diagnostics(2),
        result,
    }
}

fn fresh_observation() -> Observation {
    Observation {
        runtime_generation: 1,
        app_frame: 1,
        engine_tick: 0,
        logical_tick: 0,
        logical_elapsed_nanos: 0,
        player: [0.0; 2],
        held: [false; 4],
        score: 0,
        collected: 0,
        markers: Vec::new(),
        phase: "running".into(),
        restoring: false,
        error: None,
    }
}

fn execution_reply(state: ExecutionState) -> Reply {
    Reply::Execution {
        status: ExecutionStatus {
            state,
            input_activation: InputActivation::Isolated,
            observation: fresh_observation(),
        },
    }
}

fn resume_state() -> ResumeState {
    ResumeState {
        progress: Checkpoint::new(CheckpointLocation {
            level: LevelKey::main(LevelId::Meadow),
            anchor: SceneEntityId::new("checkpoint").unwrap(),
        }),
        player: PlayerState {
            translation: [0.0; 2],
            rotation: 0.0,
            scale: [1.0; 2],
        },
        held: [false; 4],
        clock: GameClock {
            ticks: 0,
            elapsed_nanos: 0,
            timestep_nanos: 25_000_000,
        },
        runtime_time: RuntimeTimeState {
            timestep_nanos: 25_000_000,
            max_steps_per_frame: 5,
            max_debt_steps: 120,
            catch_up_policy: RuntimeCatchUpPolicy::DiscardExcess,
            time_scale: 1.0,
            max_delta_nanos: 250_000_000,
            pending_nanos: 0,
            scaled_remainder: [u64::MAX, 9_007_199_254_740_993, (1 << 21) - 1],
        },
        counters: SessionCounters::default(),
        compatibility: ResumeCompatibility {
            schema: [1; 32],
            configuration: [2; 32],
            levels: vec![LevelContentStamp {
                level: LevelId::Meadow,
                manifest: [3; 32],
                content: [4; 32],
            }],
        },
    }
}

#[test]
fn invalid_ready_kind_preserves_diagnostics() {
    for result in [Ok(Reply::Closing), Err("startup refused".into())] {
        for previous in [None, Some(diagnostics(1))] {
            let mut cached = previous;
            let mut response = response(result.clone());
            response.request = 0;
            assert!(commit_ready_response(&mut cached, response).is_err());
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn invalid_ready_initial_state_preserves_diagnostics() {
    let fresh = fresh_observation();
    let invalid = [
        Observation {
            phase: "preparing".into(),
            ..fresh.clone()
        },
        Observation {
            restoring: true,
            ..fresh.clone()
        },
        Observation {
            error: Some("startup failed".into()),
            ..fresh.clone()
        },
        Observation {
            engine_tick: 1,
            ..fresh.clone()
        },
        Observation {
            logical_tick: 1,
            ..fresh.clone()
        },
        Observation {
            logical_elapsed_nanos: 1,
            ..fresh.clone()
        },
        Observation {
            runtime_generation: 0,
            ..fresh
        },
    ];
    for observation in invalid {
        for previous in [None, Some(diagnostics(1))] {
            let mut cached = previous;
            let mut response = response(Ok(Reply::Observed {
                observation: observation.clone(),
            }));
            response.request = 0;
            assert!(commit_ready_response(&mut cached, response).is_err());
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn valid_ready_commits_diagnostics() {
    for previous in [None, Some(diagnostics(1))] {
        let mut cached = previous;
        let mut response = response(Ok(Reply::Observed {
            observation: fresh_observation(),
        }));
        response.request = 0;
        assert_eq!(commit_ready_response(&mut cached, response).unwrap(), 1);
        assert_eq!(cached, Some(diagnostics(2)));
    }
}

#[test]
fn unexpected_exchange_reply_kind_preserves_diagnostics() {
    for (command, reply) in [
        (Command::Observe, Reply::Closing),
        (Command::ExecutionStatus, Reply::Closing),
        (
            Command::Close,
            Reply::Observed {
                observation: fresh_observation(),
            },
        ),
    ] {
        for previous in [None, Some(diagnostics(1))] {
            let mut cached = previous;
            let response = response(Ok(reply.clone()));
            assert!(
                commit_exchange_response(
                    &mut cached,
                    &mut None,
                    response.placement,
                    1,
                    &command,
                    &response,
                )
                .is_err()
            );
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn wrong_pause_and_resume_states_preserve_diagnostics() {
    for (command, state) in [
        (Command::Pause, ExecutionState::Running),
        (Command::Resume, ExecutionState::Paused),
    ] {
        for previous in [None, Some(diagnostics(1))] {
            let mut cached = previous;
            let response = response(Ok(execution_reply(state)));
            let error = commit_exchange_response(
                &mut cached,
                &mut None,
                response.placement,
                1,
                &command,
                &response,
            )
            .unwrap_err();
            assert!(error.contains("execution control returned"));
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn invalid_capture_state_preserves_diagnostics() {
    let state = resume_state();
    crate::resume::encode_resume(&state).unwrap();
    let mut invalid_position = state.clone();
    invalid_position.player.translation[0] = 281.0;
    let mut invalid_fraction = state;
    invalid_fraction.runtime_time.scaled_remainder = [0, 0, 1 << 21];
    for state in [invalid_position, invalid_fraction] {
        let response = response(Ok(Reply::Captured { state }));
        for previous in [None, Some(diagnostics(1))] {
            let mut cached = previous;
            let error = commit_exchange_response(
                &mut cached,
                &mut None,
                response.placement,
                1,
                &Command::Capture,
                &response,
            )
            .unwrap_err();
            assert!(error.contains("the game returned invalid resume state"));
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn continuity_packets_require_and_preserve_the_exact_scaled_remainder() {
    let state = resume_state();
    let request = Request {
        version: PROTOCOL_VERSION,
        session: "session".into(),
        generation: "generation".into(),
        request: 1,
        command: Command::Restore {
            state: state.clone(),
        },
    };
    let response = response(Ok(Reply::Captured { state }));
    let expected = json!([u64::MAX, 9_007_199_254_740_993_u64, (1 << 21) - 1]);

    let mut wire = serde_json::to_value(&request).unwrap();
    assert_eq!(wire["version"], PROTOCOL_VERSION);
    assert_eq!(
        wire["command"]["state"]["runtime_time"]["scaled_remainder"],
        expected
    );
    assert_eq!(
        serde_json::from_value::<Request>(wire.clone()).unwrap(),
        request
    );
    wire["command"]["state"]["runtime_time"]
        .as_object_mut()
        .unwrap()
        .remove("scaled_remainder");
    assert!(serde_json::from_value::<Request>(wire).is_err());

    let mut wire = serde_json::to_value(&response).unwrap();
    assert_eq!(wire["version"], PROTOCOL_VERSION);
    let time = &mut wire["result"]["Ok"]["state"]["runtime_time"];
    assert_eq!(time["scaled_remainder"], expected);
    assert_eq!(
        decode_response(&serde_json::to_vec(&response).unwrap()).unwrap(),
        response
    );
    time.as_object_mut().unwrap().remove("scaled_remainder");
    assert!(decode_response(&serde_json::to_vec(&wire).unwrap()).is_err());
}

#[test]
fn valid_exchange_replies_commit_diagnostics() {
    for (command, reply) in [
        (Command::Pause, execution_reply(ExecutionState::Paused)),
        (Command::Resume, execution_reply(ExecutionState::Running)),
        (
            Command::ExecutionStatus,
            execution_reply(ExecutionState::Paused),
        ),
        (
            Command::ExecutionStatus,
            execution_reply(ExecutionState::Running),
        ),
        (
            Command::Capture,
            Reply::Captured {
                state: resume_state(),
            },
        ),
        (Command::Close, Reply::Closing),
        (Command::Close, Reply::Closed { failed: false }),
        (Command::Close, Reply::Closed { failed: true }),
    ] {
        let mut cached = Some(diagnostics(1));
        let response = response(Ok(reply));
        commit_exchange_response(
            &mut cached,
            &mut None,
            response.placement,
            1,
            &command,
            &response,
        )
        .unwrap();
        assert_eq!(cached, Some(diagnostics(2)));
    }
}

#[test]
fn recoverable_error_reply_commits_diagnostics_and_preserves_error() {
    for command in [
        Command::Pause,
        Command::Resume,
        Command::Capture,
        Command::AudioStatus,
        Command::EnableAudio,
    ] {
        let mut cached = Some(diagnostics(1));
        let response = response(Err("operation refused".into()));
        let previous_audio = Some(audio_status());
        let mut audio = previous_audio;
        commit_exchange_response(
            &mut cached,
            &mut audio,
            response.placement,
            1,
            &command,
            &response,
        )
        .unwrap();
        assert_eq!(cached, Some(diagnostics(2)));
        assert_eq!(audio, previous_audio);
        assert_eq!(response.result, Err("operation refused".into()));
    }
}

fn audio_status() -> AudioStatus {
    AudioStatus {
        runtime_generation: 1,
        reset_on_rebuild: true,
        output_enabled: false,
        suppressed_effects: 3,
        accepted: 0,
        started: 0,
        completed: 0,
        cancelled: 0,
        faults: 0,
        failed: false,
        callbacks: 2,
        output_frames: 1024,
        admission_open: true,
        physical_complete: false,
        stream_drop_returned: false,
        data_callback_released: false,
        error_callback_released: false,
    }
}

#[test]
fn audio_startup_requires_reset_suppression_and_an_unused_healthy_original_output() {
    let original = audio_status();
    validate_initial_audio(original).unwrap();
    for status in [
        AudioStatus {
            reset_on_rebuild: false,
            ..original
        },
        AudioStatus {
            output_enabled: true,
            ..original
        },
        AudioStatus {
            accepted: 1,
            ..original
        },
        AudioStatus {
            faults: 1,
            ..original
        },
        AudioStatus {
            failed: true,
            ..original
        },
        AudioStatus {
            admission_open: false,
            ..original
        },
        AudioStatus {
            physical_complete: true,
            ..original
        },
        AudioStatus {
            stream_drop_returned: true,
            ..original
        },
        AudioStatus {
            data_callback_released: true,
            ..original
        },
        AudioStatus {
            error_callback_released: true,
            ..original
        },
    ] {
        assert!(validate_initial_audio(status).is_err());
    }
}

#[test]
fn audio_replies_require_the_original_placement_runtime_and_reset_policy() {
    let original = audio_status();
    for (placement, runtime, status) in [
        (DevelopmentPlacement::SourceHeadless, 1, original),
        (DevelopmentPlacement::SourceDesktop, 1, original),
        (DevelopmentPlacement::SourceDesktopAudio, 0, original),
        (DevelopmentPlacement::SourceDesktopAudio, 2, original),
        (
            DevelopmentPlacement::SourceDesktopAudio,
            1,
            AudioStatus {
                runtime_generation: 0,
                ..original
            },
        ),
        (
            DevelopmentPlacement::SourceDesktopAudio,
            1,
            AudioStatus {
                reset_on_rebuild: false,
                ..original
            },
        ),
    ] {
        for (command, reply) in [
            (Command::AudioStatus, Reply::Audio { status }),
            (Command::EnableAudio, Reply::Audio { status }),
            (
                Command::Close,
                Reply::AudioClosed {
                    failed: false,
                    status,
                },
            ),
        ] {
            let previous = Some(diagnostics(1));
            let mut cached = previous;
            let mut audio = Some(original);
            let mut response = response(Ok(reply));
            response.placement = placement;
            assert!(
                commit_exchange_response(
                    &mut cached,
                    &mut audio,
                    placement,
                    runtime,
                    &command,
                    &response,
                )
                .is_err()
            );
            assert_eq!(cached, previous);
            assert_eq!(audio, Some(original));
        }
    }
}

#[test]
fn audio_status_observes_faults_and_game_errors_without_claiming_enablement() {
    let original = audio_status();
    for status in [
        original,
        AudioStatus {
            faults: 1,
            ..original
        },
        AudioStatus {
            failed: true,
            ..original
        },
    ] {
        let mut cached = None;
        let mut audio = None;
        let mut response = response(Ok(Reply::Audio { status }));
        response.placement = DevelopmentPlacement::SourceDesktopAudio;
        commit_exchange_response(
            &mut cached,
            &mut audio,
            response.placement,
            1,
            &Command::AudioStatus,
            &response,
        )
        .unwrap();
        assert_eq!(cached, Some(diagnostics(2)));
        assert_eq!(audio, Some(status));
    }
}

#[test]
fn enable_audio_requires_an_enabled_healthy_original_live_output() {
    let enabled = AudioStatus {
        output_enabled: true,
        ..audio_status()
    };
    let mut cached = None;
    let mut audio = None;
    let mut response = response(Ok(Reply::Audio { status: enabled }));
    response.placement = DevelopmentPlacement::SourceDesktopAudio;
    commit_exchange_response(
        &mut cached,
        &mut audio,
        response.placement,
        1,
        &Command::EnableAudio,
        &response,
    )
    .unwrap();
    assert_eq!(audio, Some(enabled));
    for status in [
        AudioStatus {
            output_enabled: false,
            ..enabled
        },
        AudioStatus {
            admission_open: false,
            ..enabled
        },
        AudioStatus {
            physical_complete: true,
            ..enabled
        },
        AudioStatus {
            stream_drop_returned: true,
            ..enabled
        },
        AudioStatus {
            data_callback_released: true,
            ..enabled
        },
        AudioStatus {
            error_callback_released: true,
            ..enabled
        },
        AudioStatus {
            faults: 1,
            ..enabled
        },
        AudioStatus {
            failed: true,
            ..enabled
        },
    ] {
        response.result = Ok(Reply::Audio { status });
        assert!(
            commit_exchange_response(
                &mut cached,
                &mut audio,
                response.placement,
                1,
                &Command::EnableAudio,
                &response,
            )
            .is_err()
        );
        assert_eq!(audio, Some(enabled));
        assert_eq!(cached, Some(diagnostics(2)));
    }
}

#[test]
fn audio_close_keeps_actual_final_diagnostics_even_when_not_clean() {
    let clean = AudioStatus {
        admission_open: false,
        physical_complete: true,
        stream_drop_returned: true,
        data_callback_released: true,
        error_callback_released: true,
        ..audio_status()
    };
    assert!(clean_audio_close(clean));
    for status in [
        AudioStatus {
            output_enabled: true,
            ..clean
        },
        AudioStatus {
            admission_open: true,
            ..clean
        },
        AudioStatus {
            physical_complete: false,
            ..clean
        },
        AudioStatus {
            stream_drop_returned: false,
            ..clean
        },
        AudioStatus {
            data_callback_released: false,
            ..clean
        },
        AudioStatus {
            error_callback_released: false,
            ..clean
        },
        AudioStatus { faults: 1, ..clean },
        AudioStatus {
            failed: true,
            ..clean
        },
    ] {
        assert!(!clean_audio_close(status));
        for failed in [false, true] {
            let mut cached = None;
            let mut audio = Some(audio_status());
            let mut response = response(Ok(Reply::AudioClosed { failed, status }));
            response.placement = DevelopmentPlacement::SourceDesktopAudio;
            commit_exchange_response(
                &mut cached,
                &mut audio,
                response.placement,
                1,
                &Command::Close,
                &response,
            )
            .unwrap();
            assert_eq!(audio, Some(status));
            assert_eq!(cached, Some(diagnostics(2)));
        }
    }
}

#[test]
fn closing_and_non_audio_replies_preserve_last_audio_diagnostics() {
    for (command, reply) in [
        (Command::Close, Reply::Closing),
        (
            Command::ExecutionStatus,
            execution_reply(ExecutionState::Paused),
        ),
        (
            Command::Observe,
            Reply::Observed {
                observation: fresh_observation(),
            },
        ),
    ] {
        let original = Some(audio_status());
        let mut cached = None;
        let mut audio = original;
        let response = response(Ok(reply));
        commit_exchange_response(
            &mut cached,
            &mut audio,
            DevelopmentPlacement::SourceDesktopAudio,
            1,
            &command,
            &response,
        )
        .unwrap();
        assert_eq!(audio, original);
    }
}

#[test]
fn silent_close_cannot_complete_an_audio_placement() {
    let response = response(Ok(Reply::Closed { failed: false }));
    let mut cached = None;
    let mut audio = None;
    assert!(
        commit_exchange_response(
            &mut cached,
            &mut audio,
            DevelopmentPlacement::SourceDesktopAudio,
            1,
            &Command::Close,
            &response,
        )
        .is_err()
    );
    assert_eq!(cached, None);
    assert_eq!(audio, None);
}
