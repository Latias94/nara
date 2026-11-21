use nara::{core::SerdeShapeError, identity::SceneEntityId};
use serde_json::{Value, json};

use super::*;
use crate::{
    command::SemanticCommand,
    levels::{LevelId, LevelKey},
    resume::{GameClock, PlayerState, RuntimeCatchUpPolicy, RuntimeTimeState, SessionCounters},
    save::{
        Checkpoint, CheckpointLocation, CollectedPickup, MAX_COLLECTED_PICKUPS, MAX_MARKERS,
        MAX_OBJECT_ID_BYTES, MarkerId, MarkerRecord,
    },
};

#[test]
fn processed_round_trip_canonicalizes_game_records_and_keeps_equal_tick_order() {
    let canonical = example();
    let mut reversed = canonical.clone();
    reversed.game.progress.collected.reverse();
    reversed.game.progress.markers.reverse();

    let bytes = encode_processed_resume(&canonical).unwrap();
    assert_eq!(encode_processed_resume(&reversed).unwrap(), bytes);
    let restored = decode_processed_resume(&wire_bytes(&reversed)).unwrap();
    assert_eq!(restored, canonical);
    assert_eq!(encode_processed_resume(&restored).unwrap(), bytes);

    let digest = ContentDigest::from_parts(u64::MAX, [255; 32]);
    assert_eq!(ContentDigest::from(restored.catalog), digest);

    let mut swapped = canonical;
    swapped.pending.swap(0, 1);
    let swapped_bytes = encode_processed_resume(&swapped).unwrap();
    assert_ne!(swapped_bytes, bytes);
    assert_eq!(decode_processed_resume(&swapped_bytes).unwrap(), swapped);
}

#[test]
fn pending_limits_and_unrepresented_effects_are_refused_on_every_entry() {
    let mut state = example();
    for tick_offset in [0, MAX_TICK_OFFSET + 1] {
        state.pending[0].tick_offset = tick_offset;
        assert_rejected(&state, |error| {
            matches!(
                error,
                ProcessedResumeError::InvalidTickOffset { index: 0, tick_offset: actual }
                    if *actual == tick_offset
            )
        });
    }

    state.pending[0].tick_offset = 2;
    assert_rejected(&state, |error| {
        matches!(
            error,
            ProcessedResumeError::UnorderedTickOffset {
                index: 1,
                tick_offset: 1,
                previous_offset: 2,
            }
        )
    });

    state.pending[0].tick_offset = 1;
    state.pending[0].command = ServerCommand::Gameplay(SemanticCommand::Move {
        direction: 4,
        pressed: true,
    });
    assert_rejected(&state, |error| {
        matches!(error, ProcessedResumeError::InvalidCommand { index: 0, .. })
    });

    for command in [ServerCommand::Save, ServerCommand::Restore] {
        state.pending[0].command = command;
        assert_rejected(&state, |error| {
            matches!(error, ProcessedResumeError::SaveSlotCommand { index: 0 })
        });
    }

    state.pending =
        vec![pending(MAX_TICK_OFFSET, ServerCommand::Lookout); MAX_PENDING_SERVER_COMMANDS + 1];
    assert_rejected(&state, |error| {
        matches!(error, ProcessedResumeError::TooManyPendingCommands(count)
            if *count == MAX_PENDING_SERVER_COMMANDS + 1)
    });
}

#[test]
fn processed_game_uses_shared_state_validation() {
    let mut state = example();
    state.game.clock.elapsed_nanos += 1;
    assert_rejected(&state, |error| {
        matches!(
            error,
            ProcessedResumeError::Resume(ResumeError::ClockMismatch { .. })
        )
    });
}

#[test]
fn maximum_game_and_queue_can_reencode_after_decoding_at_the_byte_limit() {
    let mut state = example();
    let progress = &mut state.game.progress;
    let level = progress.location.level;
    progress.location.anchor = SceneEntityId::new("a".repeat(MAX_OBJECT_ID_BYTES)).unwrap();
    progress.score = MAX_COLLECTED_PICKUPS as u32 * 10;
    progress.collected = (0..MAX_COLLECTED_PICKUPS)
        .map(|index| CollectedPickup {
            level,
            entity: SceneEntityId::new(format!("{index:0width$}", width = MAX_OBJECT_ID_BYTES))
                .unwrap(),
        })
        .collect();
    progress.markers = (1..=MAX_MARKERS)
        .map(|id| marker(id as u64, level))
        .collect();
    progress.next_marker_id = MarkerId::new(MAX_MARKERS as u64 + 1).unwrap();
    state.pending = vec![
        pending(
            MAX_TICK_OFFSET,
            ServerCommand::Gameplay(SemanticCommand::Move {
                direction: 3,
                pressed: false,
            }),
        );
        MAX_PENDING_SERVER_COMMANDS
    ];

    let bytes = encode_processed_resume(&state).unwrap();
    let mut padded = bytes.clone();
    padded.resize(MAX_PROCESSED_RESUME_BYTES, b' ');
    let restored = decode_processed_resume(&padded).unwrap();
    assert_eq!(restored, state);
    assert_eq!(encode_processed_resume(&restored).unwrap(), bytes);

    padded.push(b' ');
    assert!(matches!(
        decode_processed_resume(&padded),
        Err(ProcessedResumeError::Resume(ResumeError::ByteLimitExceeded))
    ));
}

#[test]
fn wrong_format_versions_and_non_schema_fields_are_refused() {
    let bytes = encode_processed_resume(&example()).unwrap();
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(wire["kind"], "two_level_processed_resume");
    assert_eq!(wire["format_version"], 1);
    assert!(matches!(
        super::super::decode_resume(&bytes),
        Err(ResumeError::WrongKind)
    ));

    let mut invalid = wire.clone();
    invalid["format_version"] = json!(2);
    assert!(matches!(
        decode_processed_resume(&serde_json::to_vec(&invalid).unwrap()),
        Err(ProcessedResumeError::Resume(
            ResumeError::UnsupportedVersion(2)
        ))
    ));
    let mut invalid = wire.clone();
    invalid["game_compatibility"] = json!(u32::MAX);
    assert!(matches!(
        decode_processed_resume(&serde_json::to_vec(&invalid).unwrap()),
        Err(ProcessedResumeError::Resume(
            ResumeError::UnsupportedGameCompatibility(u32::MAX)
        ))
    ));

    for (path, field) in [
        ("", "unknown"),
        ("/payload", "save_slot"),
        ("/payload/game/compatibility", "levels"),
        ("/payload/catalog", "path"),
        ("/payload/pending/0", "source_sequence"),
    ] {
        let mut invalid = wire.clone();
        invalid.pointer_mut(path).unwrap()[field] = json!([]);
        assert_invalid_json(&invalid);
    }
    let mut invalid = wire.clone();
    invalid["payload"]["game"]["compatibility"] = json!([vec![0; 32], vec![0; 32]]);
    assert_invalid_json(&invalid);
    let mut invalid = wire.clone();
    invalid["payload"]["catalog"]["hash"] = json!(vec![0; 33]);
    assert_invalid_json(&invalid);
    let mut invalid = wire.clone();
    invalid["payload"]["pending"][0] = json!([1, wire["payload"]["pending"][0]["command"]]);
    assert_invalid_json(&invalid);
}

#[test]
fn pending_gameplay_command_requires_the_named_operation_object() {
    let mut wire: Value =
        serde_json::from_slice(&encode_processed_resume(&example()).unwrap()).unwrap();
    for command in [
        json!({ "gameplay": ["move", 2, true] }),
        json!({ "gameplay": ["marker"] }),
    ] {
        wire["payload"]["pending"][0]["command"] = command;
        assert_invalid_json(&wire);
    }
}

#[test]
fn structural_preflight_refuses_deep_and_duplicate_key_documents() {
    let deep = format!("{}0{}", "[".repeat(20), "]".repeat(20));
    assert!(matches!(
        decode_processed_resume(deep.as_bytes()),
        Err(ProcessedResumeError::Resume(ResumeError::Shape(
            SerdeShapeError::DepthExceeded { .. }
        )))
    ));

    let bytes = String::from_utf8(encode_processed_resume(&example()).unwrap()).unwrap();
    let duplicate = bytes.replacen(
        "\"tick_offset\":1",
        "\"tick_offset\":1,\"tick_offset\":2",
        1,
    );
    assert_ne!(duplicate, bytes);
    assert!(matches!(
        decode_processed_resume(duplicate.as_bytes()),
        Err(ProcessedResumeError::Resume(ResumeError::Shape(
            SerdeShapeError::DuplicateMapKey
        )))
    ));
}

fn assert_rejected(state: &ProcessedResumeState, check: impl Fn(&ProcessedResumeError) -> bool) {
    for result in [
        state.validate(),
        encode_processed_resume(state).map(|_| ()),
        decode_processed_resume(&wire_bytes(state)).map(|_| ()),
    ] {
        let error = result.unwrap_err();
        assert!(check(&error), "{error:?}");
    }
}

fn assert_invalid_json(wire: &Value) {
    assert!(matches!(
        decode_processed_resume(&serde_json::to_vec(wire).unwrap()),
        Err(ProcessedResumeError::Resume(ResumeError::Json(_)))
    ));
}

fn wire_bytes(state: &ProcessedResumeState) -> Vec<u8> {
    serde_json::to_vec(&Envelope {
        kind: KIND,
        format_version: FORMAT_VERSION,
        game_compatibility: GAME_COMPATIBILITY,
        payload: state,
    })
    .unwrap()
}

fn example() -> ProcessedResumeState {
    let level = LevelKey::main(LevelId::Meadow);
    ProcessedResumeState {
        game: ResumeState {
            progress: Checkpoint {
                location: CheckpointLocation {
                    level,
                    anchor: SceneEntityId::new("checkpoint").unwrap(),
                },
                score: 20,
                collected: ["gem-a", "gem-b"]
                    .into_iter()
                    .map(|entity| CollectedPickup {
                        level,
                        entity: SceneEntityId::new(entity).unwrap(),
                    })
                    .collect(),
                markers: vec![marker(1, level), marker(2, level)],
                next_marker_id: MarkerId::new(3).unwrap(),
            },
            player: PlayerState {
                translation: [17.25, -31.5],
                rotation: 0.5,
                scale: [-1.5, 2.0],
            },
            held: [true, false, false, true],
            clock: GameClock {
                ticks: 12,
                elapsed_nanos: 300_000_000,
                timestep_nanos: 25_000_000,
            },
            runtime_time: RuntimeTimeState {
                timestep_nanos: 25_000_000,
                max_steps_per_frame: 5,
                max_debt_steps: 120,
                catch_up_policy: RuntimeCatchUpPolicy::PreserveDebt,
                time_scale: 1.0,
                max_delta_nanos: 250_000_000,
                pending_nanos: 38_000_000,
            },
            counters: SessionCounters {
                travel_count: 3,
                restarts: 2,
                saves: 4,
                restores: 1,
            },
            compatibility: ProcessedResumeCompatibility {
                schema: [9; 32],
                configuration: [10; 32],
            },
        },
        catalog: ContentDigest::from_parts(u64::MAX, [255; 32]).into(),
        pending: vec![
            pending(
                1,
                ServerCommand::Gameplay(SemanticCommand::Move {
                    direction: 2,
                    pressed: true,
                }),
            ),
            pending(1, ServerCommand::SideRoom),
            pending(MAX_TICK_OFFSET, ServerCommand::Lookout),
        ],
    }
}

fn marker(id: u64, level: LevelKey) -> MarkerRecord {
    MarkerRecord {
        id: MarkerId::new(id).unwrap(),
        level,
        position: [40.0, 12.5],
    }
}

fn pending(tick_offset: u64, command: ServerCommand) -> PendingServerCommand {
    PendingServerCommand {
        tick_offset,
        command,
    }
}
