use nara::{
    app::{App, CoreStage, FixedTime, RuntimeInstance},
    ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs},
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPlugin, GameplayCommandQueue, GameplayCommandSet,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTarget,
        GameplayCommandTick, GameplayCommandValue,
    },
    identity::SceneEntityId,
};
use serde_json::json;

use super::*;
use crate::{
    gameplay::commands,
    levels::{LevelId, LevelKey},
    resume::{RuntimeCatchUpPolicy, decode_resume},
    save::CheckpointLocation,
};

fn state(tick: u64) -> RecordedState {
    RecordedState {
        progress: Checkpoint::new(CheckpointLocation {
            level: LevelKey::main(LevelId::STARTUP),
            anchor: SceneEntityId::new("checkpoint").unwrap(),
        }),
        player: PlayerState {
            translation: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        },
        held: [false; 4],
        clock: GameClock {
            ticks: tick,
            elapsed_nanos: tick * 25_000_000,
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
        schema: [9; 32],
        levels: vec![stamp(LevelId::STARTUP)],
    }
}

fn stamp(level: LevelId) -> LevelContentStamp {
    LevelContentStamp {
        level,
        manifest: [4; 32],
        content: match level {
            LevelId::Meadow => [5; 32],
            LevelId::Cavern => [7; 32],
        },
    }
}

fn key(tick: u64, sequence: u64) -> GameplayCommandKey {
    GameplayCommandKey::new(
        GameplayCommandTick::new(tick).unwrap(),
        GameplayCommandSource::external(RECORDING_SOURCE).unwrap(),
        GameplayCommandSourceSequence::new(sequence).unwrap(),
    )
}

fn recording() -> GameRecording {
    let steps = (1..=3)
        .map(|tick| {
            let mut after = state(tick);
            after.levels.push(stamp(LevelId::Cavern));
            after.held[1] = tick < 3;
            let commands = match tick {
                1 | 3 => vec![RecordedCommand {
                    command: SemanticCommand::Move {
                        direction: 1,
                        pressed: tick == 1,
                    },
                    original: key(tick, if tick == 1 { 5 } else { 8 }),
                }],
                _ => Vec::new(),
            };
            RecordedStep {
                tick,
                commands,
                after,
            }
        })
        .collect();
    GameRecording {
        format: CURRENT_FORMAT_VERSION,
        configuration: [10; 32],
        initial: state(0),
        steps,
    }
}

#[derive(Default, Resource)]
struct Captured(Vec<GameplayCommandEnvelope>);

fn capture_commands(batch: Res<GameplayCommandBatch>, mut captured: ResMut<Captured>) {
    captured.0.extend_from_slice(batch.commands());
}

fn capture(drafts: Vec<GameplayCommandDraft>) -> Vec<GameplayCommandEnvelope> {
    let mut app = App::new();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    app.init_resource::<Captured>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_commands.in_set(GameplayCommandSet::Capture),
    )
    .unwrap();
    for (index, draft) in drafts.into_iter().enumerate() {
        app.world_mut()
            .unwrap()
            .resource_mut::<GameplayCommandQueue>()
            .submit(GameplayCommandSubmission::new(
                GameplayCommandTick::new(1).unwrap(),
                GameplayCommandIngressSource::external(RECORDING_SOURCE).unwrap(),
                GameplayCommandSourceSequence::new(index as u64 + 1).unwrap(),
                draft,
            ))
            .unwrap();
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let captured = runtime.world().resource::<Captured>().0.clone();
    runtime.finish().unwrap();
    captured
}

#[test]
fn shared_semantics_preserve_protocol_wire_and_actual_capture_keys() {
    let commands = [
        (
            SemanticCommand::Move {
                direction: 3,
                pressed: true,
            },
            json!({"operation": "move", "direction": 3, "pressed": true}),
        ),
        (SemanticCommand::Marker, json!({"operation": "marker"})),
        (SemanticCommand::Travel, json!({"operation": "travel"})),
        (SemanticCommand::Instance, json!({"operation": "instance"})),
        (SemanticCommand::Retry, json!({"operation": "retry"})),
    ];
    let captured = capture(
        commands
            .iter()
            .map(|(command, _)| command.draft())
            .collect(),
    );
    assert_eq!(captured.len(), commands.len());
    for ((command, wire), envelope) in commands.iter().zip(&captured) {
        assert_eq!(serde_json::to_value(command).unwrap(), *wire);
        assert_eq!(
            serde_json::from_value::<SemanticCommand>(wire.clone()).unwrap(),
            *command,
        );
        let recorded = RecordedCommand::from_envelope(envelope).unwrap();
        assert_eq!(recorded.command, *command);
        assert_eq!(&recorded.original, envelope.key());
        assert_eq!(recorded.command.draft(), *envelope.command());
    }
}

#[test]
fn capture_refuses_targets_unknown_actions_and_unrepresented_or_mistyped_payloads() {
    let mut extra = commands::movement(0, true).payload().clone();
    extra
        .insert("extra", GameplayCommandValue::Bool(false))
        .unwrap();
    let mut negative = commands::movement(0, true).payload().clone();
    negative
        .insert("direction", GameplayCommandValue::I64(-1))
        .unwrap();
    let mut mistyped = commands::movement(0, true).payload().clone();
    mistyped
        .insert("pressed", GameplayCommandValue::I64(1))
        .unwrap();
    let invalid = vec![
        commands::action(commands::SAVE),
        commands::action(commands::RESTORE),
        commands::action(commands::PLAYER_LAUNCH),
        commands::action("two-level-game.unknown"),
        commands::action(commands::MOVE),
        commands::movement(4, true),
        commands::movement(0, true).with_payload(extra.clone()),
        commands::movement(0, true).with_payload(negative),
        commands::movement(0, true).with_payload(mistyped),
        commands::action(commands::MARKER).with_payload(extra),
        commands::action(commands::RETRY)
            .with_target(GameplayCommandTarget::named("player").unwrap()),
    ];
    let count = invalid.len();
    let captured = capture(invalid);
    assert_eq!(captured.len(), count);
    for envelope in captured {
        assert!(
            RecordedCommand::from_envelope(&envelope).is_err(),
            "accepted unsupported Capture envelope: {envelope:?}",
        );
    }
}

#[test]
fn state_comparison_reuses_resume_canonicalization_and_omits_recipe_configuration() {
    let mut left = decode_resume(&state(0).canonical_bytes().unwrap()).unwrap();
    left.compatibility.configuration = [1; 32];
    left.compatibility.levels.push(stamp(LevelId::Cavern));
    let mut right = left.clone();
    right.compatibility.configuration = [2; 32];
    right.compatibility.levels.reverse();
    let canonical = RecordedState::from(left).canonical_bytes().unwrap();
    assert_eq!(
        RecordedState::from(right).canonical_bytes().unwrap(),
        canonical,
    );
    assert_eq!(
        decode_resume(&canonical)
            .unwrap()
            .compatibility
            .configuration,
        [0; 32],
    );
    let mut invalid = state(0);
    invalid.progress.score = 10;
    assert!(invalid.canonical_bytes().is_err());
}

#[test]
fn recording_round_trip_keeps_provenance_without_serializing_another_payload() {
    let recording = recording();
    let bytes = encode_recording(&recording).unwrap();
    assert_eq!(decode_recording(&bytes).unwrap(), recording);
    let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        wire["steps"][0]["commands"][0],
        json!({
            "command": {"operation": "move", "direction": 1, "pressed": true},
            "original": {
                "tick": 1,
                "source": {"External": {"producer": RECORDING_SOURCE}},
                "source_sequence": 5,
            },
        }),
    );
}
