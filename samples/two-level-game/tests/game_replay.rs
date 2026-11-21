#![cfg(feature = "development")]
#![forbid(unsafe_code)]
#![cfg(feature = "development")]

#[path = "support/game_replay.rs"]
mod support;

use std::fs;

use nara::gameplay::{GameplayCommandKey, GameplayCommandSource, GameplayCommandTick};
use support::{ProjectCopy, full_program};
use two_level_game::{
    command::SemanticCommand,
    levels::{LevelId, LevelKey},
    replay::{
        GameProgram, GameRecording, RecordedState, decode_program, decode_recording,
        encode_program, encode_recording, record_program, replay_recording,
    },
};

fn command_indices(recording: &GameRecording, command: SemanticCommand) -> Vec<usize> {
    recording
        .steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            step.commands
                .iter()
                .any(|captured| captured.command == command)
                .then_some(index)
        })
        .collect()
}

fn marker_ids(state: &RecordedState, level: LevelKey) -> Vec<u64> {
    state
        .progress
        .markers
        .iter()
        .filter(|marker| marker.level == level)
        .map(|marker| marker.id.get())
        .collect()
}

#[test]
fn public_recording_round_trips_and_replays_the_complete_game_in_a_fresh_project() {
    let project = ProjectCopy::new();
    let program_bytes = encode_program(&full_program()).unwrap();
    let program = decode_program(&program_bytes).unwrap();
    assert_eq!(encode_program(&program).unwrap(), program_bytes);
    let recording = record_program(project.root(), &program).expect("actual source-game recording");
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let second_cavern = LevelKey::new(LevelId::Cavern, 1).unwrap();
    let initial = &recording.initial;
    assert_eq!(initial.progress.location.level, meadow);
    assert_eq!(initial.progress.score, 0);
    assert!(initial.progress.collected.is_empty());
    assert!(initial.progress.markers.is_empty());
    assert_eq!(initial.progress.next_marker_id.get(), 1);
    assert_eq!(initial.player.translation, [-120.0, 0.0]);
    assert_eq!(initial.held, [false; 4]);
    assert_eq!(initial.clock.ticks, 0);
    assert_eq!(initial.clock.elapsed_nanos, 0);
    assert_eq!(initial.clock.timestep_nanos, 20_000_000);
    assert_eq!(recording.steps.len(), 71);
    assert_eq!(recording.steps.len(), program.steps().len());

    let expected_source = GameplayCommandSource::external("two-level-game.recording").unwrap();
    let mut sequence = 0;
    let mut previous = initial;
    for (index, step) in recording.steps.iter().enumerate() {
        assert_eq!(step.tick, index as u64 + 1);
        assert_eq!(step.commands.len(), program.steps()[index].len());
        for (captured, expected) in step.commands.iter().zip(&program.steps()[index]) {
            assert_eq!(&captured.command, expected);
        }
        for captured in &step.commands {
            sequence += 1;
            assert_eq!(captured.original.tick().get(), step.tick);
            assert_eq!(captured.original.source(), &expected_source);
            assert_eq!(captured.original.source_sequence().get(), sequence);
        }
        let loading = step.commands.iter().any(|captured| {
            matches!(
                captured.command,
                SemanticCommand::Travel | SemanticCommand::Instance
            )
        });
        assert_eq!(
            step.after.clock.ticks,
            previous.clock.ticks + u64::from(!loading),
            "row {} must settle loading without an extra simulation tick",
            step.tick
        );
        assert_eq!(
            step.after.clock.elapsed_nanos,
            step.after.clock.ticks * step.after.clock.timestep_nanos
        );
        assert_eq!(step.after.runtime_time, initial.runtime_time);
        assert_eq!(step.after.schema, initial.schema);
        assert_eq!(step.after.counters.saves, 0);
        assert_eq!(step.after.counters.restores, 0);
        previous = &step.after;
    }
    assert_eq!(sequence, 12);

    let walks = command_indices(
        &recording,
        SemanticCommand::Move {
            direction: 1,
            pressed: true,
        },
    );
    assert_eq!(walks.len(), 3);
    for index in walks {
        let pressed = &recording.steps[index].after;
        let continued = &recording.steps[index + 1];
        assert_eq!(pressed.held, [false, true, false, false]);
        assert!(continued.commands.is_empty());
        assert_eq!(continued.after.held, pressed.held);
        assert!(continued.after.player.translation[0] > pressed.player.translation[0]);
    }
    let release = command_indices(
        &recording,
        SemanticCommand::Move {
            direction: 1,
            pressed: false,
        },
    );
    assert_eq!(release.len(), 1);
    let released = &recording.steps[release[0]].after;
    let idle = &recording.steps[release[0] + 1];
    assert_eq!(released.held, [false; 4]);
    assert!(idle.commands.is_empty());
    assert_eq!(idle.after.held, [false; 4]);
    assert_eq!(idle.after.player, released.player);

    let markers = command_indices(&recording, SemanticCommand::Marker);
    assert_eq!(markers.len(), 3);
    for (ordinal, (index, level)) in markers
        .into_iter()
        .zip([meadow, cavern, second_cavern])
        .enumerate()
    {
        let state = &recording.steps[index].after;
        assert_eq!(state.progress.location.level, level);
        assert_eq!(state.progress.score, (ordinal as u32 + 1) * 10);
        assert_eq!(state.progress.collected.len(), ordinal + 1);
        assert_eq!(state.progress.markers.len(), ordinal + 1);
        assert_eq!(marker_ids(state, level), [ordinal as u64 + 1]);
        assert_eq!(state.progress.next_marker_id.get(), ordinal as u64 + 2);
        assert_eq!(state.held, [false, true, false, false]);
        let marker = state
            .progress
            .markers
            .iter()
            .find(|marker| marker.id.get() == ordinal as u64 + 1)
            .unwrap();
        assert_eq!(marker.level, level);
        assert_eq!(
            marker.position,
            recording.steps[index - 1].after.player.translation
        );
    }

    let travels = command_indices(&recording, SemanticCommand::Travel);
    assert_eq!(travels.len(), 2);
    let entered_cavern = &recording.steps[travels[0]].after;
    assert_eq!(entered_cavern.progress.location.level, cavern);
    assert_eq!(entered_cavern.progress.score, 10);
    assert_eq!(entered_cavern.counters.travel_count, 1);
    assert_eq!(entered_cavern.held, [false; 4]);
    assert_eq!(entered_cavern.player.translation, [-120.0, 0.0]);

    let instances = command_indices(&recording, SemanticCommand::Instance);
    assert_eq!(instances.len(), 2);
    for (index, level, count, score) in [
        (instances[0], second_cavern, 2, 20),
        (instances[1], cavern, 3, 30),
    ] {
        let state = &recording.steps[index].after;
        assert_eq!(state.progress.location.level, level);
        assert_eq!(state.progress.score, score);
        assert_eq!(state.counters.travel_count, count);
        assert_eq!(state.held, [false; 4]);
        assert_eq!(state.player.translation, [-120.0, 0.0]);
    }
    let retries = command_indices(&recording, SemanticCommand::Retry);
    assert_eq!(retries.len(), 1);
    let retried = &recording.steps[retries[0]].after;
    assert_eq!(
        retried.progress,
        recording.steps[instances[1]].after.progress
    );
    assert_eq!(retried.counters.travel_count, 3);
    assert_eq!(retried.counters.restarts, 1);
    assert_eq!(retried.held, [false; 4]);
    assert_eq!(retried.player.translation, [-120.0, 0.0]);

    let final_state = &recording.steps.last().unwrap().after;
    assert_eq!(final_state.progress.location.level, meadow);
    assert_eq!(final_state.progress.location.anchor.as_str(), "checkpoint");
    assert_eq!(final_state.progress.score, 30);
    assert_eq!(final_state.progress.collected.len(), 3);
    assert_eq!(final_state.progress.markers.len(), 3);
    assert_eq!(final_state.progress.next_marker_id.get(), 4);
    for (level, id) in [(meadow, 1), (cavern, 2), (second_cavern, 3)] {
        let pickups = final_state
            .progress
            .collected
            .iter()
            .filter(|pickup| pickup.level == level)
            .collect::<Vec<_>>();
        assert_eq!(pickups.len(), 1);
        assert_eq!(pickups[0].entity.as_str(), "gem-near");
        assert_eq!(marker_ids(final_state, level), [id]);
    }
    assert_eq!(final_state.counters.travel_count, 4);
    assert_eq!(final_state.counters.restarts, 1);
    assert_eq!(final_state.held, [false; 4]);
    assert_eq!(final_state.clock.ticks, 67);
    assert_eq!(final_state.levels.len(), 2);
    assert!(
        final_state
            .levels
            .iter()
            .any(|stamp| stamp.level == LevelId::Meadow)
    );
    assert!(
        final_state
            .levels
            .iter()
            .any(|stamp| stamp.level == LevelId::Cavern)
    );

    let recording_bytes = encode_recording(&recording).unwrap();
    let path = project.root().parent().unwrap().join("recording.json");
    fs::write(&path, &recording_bytes).unwrap();
    let decoded = decode_recording(&fs::read(path).unwrap()).unwrap();
    assert_eq!(encode_recording(&decoded).unwrap(), recording_bytes);
    assert_eq!(
        decoded.initial.canonical_bytes().unwrap(),
        initial.canonical_bytes().unwrap()
    );
    for (original, decoded) in recording.steps.iter().zip(&decoded.steps) {
        assert_eq!(original.tick, decoded.tick);
        assert_eq!(original.commands.len(), decoded.commands.len());
        for (original, decoded) in original.commands.iter().zip(&decoded.commands) {
            assert_eq!(original.command, decoded.command);
            assert_eq!(original.original, decoded.original);
        }
        assert_eq!(
            original.after.canonical_bytes().unwrap(),
            decoded.after.canonical_bytes().unwrap()
        );
    }
    let replay_project = ProjectCopy::new();
    assert_ne!(project.root(), replay_project.root());
    let report =
        replay_recording(replay_project.root(), &decoded).expect("fresh source-game replay");
    assert_eq!(report.steps, recording.steps.len());
    assert_eq!(report.commands, sequence as usize);
    assert_eq!(
        report.final_state.canonical_bytes().unwrap(),
        final_state.canonical_bytes().unwrap()
    );
}

#[test]
fn replay_refuses_a_tampered_intermediate_result_even_when_the_final_result_is_unchanged() {
    let project = ProjectCopy::new();
    let program = GameProgram::new(vec![
        vec![SemanticCommand::Move {
            direction: 1,
            pressed: true,
        }],
        vec![],
        vec![SemanticCommand::Move {
            direction: 1,
            pressed: false,
        }],
    ])
    .unwrap();
    let recording = record_program(project.root(), &program).unwrap();
    let mut tampered = decode_recording(&encode_recording(&recording).unwrap()).unwrap();
    tampered.steps[0].after.player.translation[0] += 1.0;
    assert_eq!(
        tampered
            .steps
            .last()
            .unwrap()
            .after
            .canonical_bytes()
            .unwrap(),
        recording
            .steps
            .last()
            .unwrap()
            .after
            .canonical_bytes()
            .unwrap()
    );
    let imported = decode_recording(&encode_recording(&tampered).unwrap()).unwrap();
    let error = replay_recording(project.root(), &imported).unwrap_err();
    assert!(error.to_string().contains("state mismatch"), "{error}");
    assert!(error.to_string().contains("step 1"), "{error}");
    assert!(!error.cleanup_pending(), "{error:?}");
    let report = replay_recording(project.root(), &recording).expect("fresh replay after refusal");
    assert_eq!(
        report.final_state.canonical_bytes().unwrap(),
        recording
            .steps
            .last()
            .unwrap()
            .after
            .canonical_bytes()
            .unwrap()
    );
}

#[test]
fn replay_refuses_valid_scene_content_drift_without_relying_on_a_gameplay_difference() {
    let project = ProjectCopy::new();
    let program = GameProgram::new(vec![vec![SemanticCommand::Travel], vec![]]).unwrap();
    let recording = record_program(project.root(), &program).unwrap();
    let path = project.root().join("scenes/cavern.scene.json");
    let mut scene: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let pickup = scene["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == "gem-far")
        .unwrap();
    let x = &mut pickup["components"]["nara.transform.Transform2d"]["value"]["value"]["translation"]
        ["value"]["x"]["value"];
    assert_eq!(x.as_f64(), Some(80.0));
    *x = serde_json::json!(81);
    fs::write(path, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();
    let changed = record_program(project.root(), &program).expect("the edited Scene is valid");
    let original_state = &recording.steps.last().unwrap().after;
    let changed_state = &changed.steps.last().unwrap().after;
    assert_eq!(original_state.progress, changed_state.progress);
    assert_eq!(original_state.player, changed_state.player);
    assert_eq!(original_state.clock, changed_state.clock);
    assert_ne!(original_state.levels, changed_state.levels);
    let error = replay_recording(project.root(), &recording).unwrap_err();
    assert!(error.to_string().contains("state mismatch"), "{error}");
    assert!(!error.cleanup_pending(), "{error:?}");
    assert!(replay_recording(project.root(), &changed).is_ok());
}

#[test]
fn recording_refuses_an_unloadable_destination_and_allows_a_fresh_attempt_after_repair() {
    let project = ProjectCopy::new();
    let program = GameProgram::new(vec![vec![SemanticCommand::Travel], vec![]]).unwrap();
    let path = project.root().join("scenes/cavern.scene.json");
    let source = fs::read(&path).unwrap();
    fs::write(&path, b"{ malformed destination Scene").unwrap();
    let error = record_program(project.root(), &program).unwrap_err();
    assert!(!error.cleanup_pending(), "{error:?}");
    fs::write(path, source).unwrap();
    let recording = record_program(project.root(), &program).expect("fresh attempt after repair");
    assert_eq!(recording.steps.len(), 2);
    assert_eq!(
        recording.steps[0].after.progress.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert!(replay_recording(project.root(), &recording).is_ok());
}

#[test]
fn invalid_move_direction_is_rejected_by_program_construction_before_any_runtime_exists() {
    for pressed in [true, false] {
        assert!(
            GameProgram::new(vec![vec![SemanticCommand::Move {
                direction: 4,
                pressed,
            }]])
            .is_err()
        );
    }
}

#[test]
fn recording_preserves_multiple_commands_in_each_original_capture_order() {
    let project = ProjectCopy::new();
    let rows = vec![
        vec![
            SemanticCommand::Move {
                direction: 1,
                pressed: true,
            },
            SemanticCommand::Marker,
        ],
        vec![
            SemanticCommand::Move {
                direction: 1,
                pressed: false,
            },
            SemanticCommand::Move {
                direction: 2,
                pressed: true,
            },
        ],
        vec![],
        vec![SemanticCommand::Move {
            direction: 2,
            pressed: false,
        }],
    ];
    let program = GameProgram::new(rows.clone()).unwrap();
    let recording = record_program(project.root(), &program).unwrap();
    assert_eq!(recording.steps.len(), rows.len());
    let mut sequence = 0;
    for (row, expected) in recording.steps.iter().zip(&rows) {
        assert_eq!(row.commands.len(), expected.len());
        for (actual, expected) in row.commands.iter().zip(expected) {
            sequence += 1;
            assert_eq!(&actual.command, expected);
            assert_eq!(actual.original.tick().get(), row.tick);
            assert_eq!(actual.original.source_sequence().get(), sequence);
        }
    }
    let first = &recording.steps[0].after;
    let changed = &recording.steps[1].after;
    let held = &recording.steps[2].after;
    let released = &recording.steps[3].after;
    assert_eq!(first.progress.markers.len(), 1);
    assert_eq!(
        first.progress.markers[0].position,
        recording.initial.player.translation
    );
    assert_eq!(first.held, [false, true, false, false]);
    assert_eq!(changed.held, [false, false, true, false]);
    assert_eq!(changed.player.translation[0], first.player.translation[0]);
    assert!(changed.player.translation[1] > first.player.translation[1]);
    assert_eq!(held.held, changed.held);
    assert!(held.player.translation[1] > changed.player.translation[1]);
    assert_eq!(released.held, [false; 4]);
    assert_eq!(released.player, held.player);
    let report = replay_recording(project.root(), &recording).unwrap();
    assert_eq!(report.commands, 5);
    assert_eq!(
        report.final_state.canonical_bytes().unwrap(),
        released.canonical_bytes().unwrap()
    );
}

#[test]
fn program_and_recording_import_refuse_save_restore_unknown_operations_and_unknown_fields() {
    let program = GameProgram::new(vec![vec![SemanticCommand::Marker]]).unwrap();
    let program_bytes = encode_program(&program).unwrap();
    assert!(decode_program(&program_bytes).is_ok());
    let program_json: serde_json::Value = serde_json::from_slice(&program_bytes).unwrap();
    let project = ProjectCopy::new();
    let recording = record_program(project.root(), &program).unwrap();
    let recording_bytes = encode_recording(&recording).unwrap();
    let recording_json: serde_json::Value = serde_json::from_slice(&recording_bytes).unwrap();
    for command in [
        serde_json::json!({"operation": "save"}),
        serde_json::json!({"operation": "restore"}),
        serde_json::json!({"operation": "unknown"}),
        serde_json::json!({"operation": "marker", "payload": {}}),
        serde_json::json!({"operation": "move", "direction": 4, "pressed": true}),
        serde_json::json!({"operation": "move", "direction": 1}),
    ] {
        let mut program_json = program_json.clone();
        program_json["steps"][0][0] = command.clone();
        assert!(decode_program(&serde_json::to_vec(&program_json).unwrap()).is_err());
        let mut recording_json = recording_json.clone();
        recording_json["steps"][0]["commands"][0]["command"] = command;
        assert!(decode_recording(&serde_json::to_vec(&recording_json).unwrap()).is_err());
    }
    let mut program_json = program_json;
    program_json["unknown"] = serde_json::json!(true);
    assert!(decode_program(&serde_json::to_vec(&program_json).unwrap()).is_err());
    let mut recording_json = recording_json;
    recording_json["unknown"] = serde_json::json!(true);
    assert!(decode_recording(&serde_json::to_vec(&recording_json).unwrap()).is_err());
}

#[test]
fn noncontiguous_ticks_and_mismatched_capture_keys_are_refused_before_playback_startup() {
    let project = ProjectCopy::new();
    let program = GameProgram::new(vec![vec![SemanticCommand::Marker], vec![]]).unwrap();
    let recording = record_program(project.root(), &program).unwrap();
    let mut skipped = recording.clone();
    skipped.steps[1].tick += 1;
    let mut repeated = recording.clone();
    repeated.steps[1].tick = repeated.steps[0].tick;
    let mut mismatched = recording;
    let original = &mismatched.steps[0].commands[0].original;
    mismatched.steps[0].commands[0].original = GameplayCommandKey::new(
        GameplayCommandTick::new(2).unwrap(),
        original.source().clone(),
        original.source_sequence(),
    );
    let absent_project = project.root().join("project-that-does-not-exist");
    assert!(!absent_project.exists());
    for refused in [skipped, repeated, mismatched] {
        assert!(encode_recording(&refused).is_err());
        assert!(decode_recording(&serde_json::to_vec(&refused).unwrap()).is_err());
        let expected = refused.validate().unwrap_err();
        let error = replay_recording(&absent_project, &refused).unwrap_err();
        assert_eq!(error.to_string(), expected);
        assert!(!error.cleanup_pending(), "{error:?}");
    }
}

#[test]
fn program_and_recording_import_enforce_versions_and_the_documented_byte_boundary() {
    const FILE_BYTE_LIMIT: usize = 16 * 1024 * 1024;

    let program = GameProgram::new(vec![vec![]]).unwrap();
    let program_bytes = encode_program(&program).unwrap();
    let program_json: serde_json::Value = serde_json::from_slice(&program_bytes).unwrap();
    let project = ProjectCopy::new();
    let recording = record_program(project.root(), &program).unwrap();
    let recording_bytes = encode_recording(&recording).unwrap();
    let recording_json: serde_json::Value = serde_json::from_slice(&recording_bytes).unwrap();
    assert_eq!(program_json["format"], 1);
    assert_eq!(recording_json["format"], 2);
    for version in [0, 2] {
        let mut program_json = program_json.clone();
        program_json["format"] = serde_json::json!(version);
        assert!(decode_program(&serde_json::to_vec(&program_json).unwrap()).is_err());
    }
    for version in [0, 1, 3] {
        let mut recording_json = recording_json.clone();
        recording_json["format"] = serde_json::json!(version);
        assert!(decode_recording(&serde_json::to_vec(&recording_json).unwrap()).is_err());
        let mut recording = recording.clone();
        recording.format = version;
        assert!(encode_recording(&recording).is_err());
    }

    let mut exact_program = program_bytes;
    exact_program.resize(FILE_BYTE_LIMIT, b' ');
    assert!(decode_program(&exact_program).is_ok());
    exact_program.push(b' ');
    assert!(decode_program(&exact_program).is_err());
    drop(exact_program);
    let mut exact_recording = recording_bytes;
    exact_recording.resize(FILE_BYTE_LIMIT, b' ');
    assert!(decode_recording(&exact_recording).is_ok());
    exact_recording.push(b' ');
    assert!(decode_recording(&exact_recording).is_err());
}
