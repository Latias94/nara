use nara::identity::SceneEntityId;

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
        watcher_joined: false,
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
        commit_ready_response(&mut cached, response).unwrap();
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
            assert!(commit_exchange_response(&mut cached, &command, &response).is_err());
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
            let error = commit_exchange_response(&mut cached, &command, &response).unwrap_err();
            assert!(error.contains("execution control returned"));
            assert_eq!(cached, previous);
        }
    }
}

#[test]
fn invalid_capture_state_preserves_diagnostics() {
    let mut state = resume_state();
    crate::resume::encode_resume(&state).unwrap();
    state.player.translation[0] = 281.0;
    let response = response(Ok(Reply::Captured { state }));
    for previous in [None, Some(diagnostics(1))] {
        let mut cached = previous;
        let error =
            commit_exchange_response(&mut cached, &Command::Capture, &response).unwrap_err();
        assert!(error.contains("the game returned invalid resume state"));
        assert_eq!(cached, previous);
    }
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
        commit_exchange_response(&mut cached, &command, &response).unwrap();
        assert_eq!(cached, Some(diagnostics(2)));
    }
}

#[test]
fn recoverable_error_reply_commits_diagnostics_and_preserves_error() {
    for command in [Command::Pause, Command::Resume, Command::Capture] {
        let mut cached = Some(diagnostics(1));
        let response = response(Err("operation refused".into()));
        commit_exchange_response(&mut cached, &command, &response).unwrap();
        assert_eq!(cached, Some(diagnostics(2)));
        assert_eq!(response.result, Err("operation refused".into()));
    }
}
