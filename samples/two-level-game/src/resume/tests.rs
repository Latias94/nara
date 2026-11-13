use nara::identity::SceneEntityId;
use serde_json::{Value, json};

use super::*;
use crate::{
    levels::LevelKey,
    save::{
        CheckpointLocation, CollectedPickup, MAX_COLLECTED_PICKUPS, MAX_MARKER_COORDINATE,
        MAX_MARKERS, MAX_SCORE, MarkerId, MarkerRecord, decode_checkpoint, encode_checkpoint,
    },
};

#[test]
fn canonical_resume_has_an_exact_round_trip() {
    let state = example();
    let bytes = encode_resume(&state).unwrap();
    let expected = concat!(
        r#"{"kind":"two_level_resume","format_version":1,"game_compatibility":1,"payload":{"progress":{"#,
        r#""location":{"level":{"level":"cavern","instance":2},"anchor":"checkpoint"},"score":20,"#,
        r#""collected":[{"level":{"level":"meadow","instance":0},"entity":"gem-near"},"#,
        r#"{"level":{"level":"cavern","instance":2},"entity":"gem-away"}],"#,
        r#""markers":[{"id":1,"level":{"level":"meadow","instance":0},"position":[-80.0,0.0]},"#,
        r#"{"id":2,"level":{"level":"cavern","instance":2},"position":[40.0,12.5]}],"next_marker_id":3},"#,
        r#""player":{"translation":[17.25,-31.5],"rotation":0.5,"scale":[-1.5,2.0]},"#,
        r#""held":[true,false,false,true],"clock":{"ticks":12,"elapsed_nanos":300000000,"timestep_nanos":25000000},"#,
        r#""counters":{"travel_count":3,"restarts":2,"saves":4,"restores":1},"compatibility":{"schema":"#,
        "[9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9],",
        r#""configuration":"#,
        "[10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10,10],",
        r#""levels":[{"level":"meadow","manifest":"#,
        "[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4],",
        r#""content":"#,
        "[5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5]},",
        r#"{"level":"cavern","manifest":"#,
        "[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4],",
        r#""content":"#,
        "[7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7]}]}}}",
    );
    assert_eq!(bytes.as_slice(), expected.as_bytes());
    assert_eq!(decode_resume(&bytes).unwrap(), state);
    assert_eq!(
        encode_resume(&decode_resume(&bytes).unwrap()).unwrap(),
        bytes
    );
}

#[test]
fn set_order_does_not_affect_encoding_and_decoding_returns_canonical_state() {
    let canonical = example();
    let mut reversed = canonical.clone();
    reversed.progress.collected.reverse();
    reversed.progress.markers.reverse();
    reversed.compatibility.levels.reverse();
    let unchanged = reversed.clone();

    assert_eq!(
        encode_resume(&reversed).unwrap(),
        encode_resume(&canonical).unwrap()
    );
    assert_eq!(reversed, unchanged);
    assert_eq!(decode_resume(&wire_bytes(&reversed)).unwrap(), canonical);
}

#[test]
fn existing_progress_validation_is_propagated() {
    let mut state = example();
    state.progress.score = MAX_SCORE + 1;
    assert!(matches!(
        state.validate(),
        Err(ResumeError::Progress(CheckpointError::ScoreOutOfRange))
    ));

    state = example();
    state
        .progress
        .collected
        .push(state.progress.collected[0].clone());
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::Progress(
            CheckpointError::DuplicateCollectedPickup
        ))
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::Progress(
            CheckpointError::DuplicateCollectedPickup
        ))
    ));

    state = example();
    state.progress.markers[0].position[0] = f32::NAN;
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::Progress(
            CheckpointError::NonFiniteMarkerPosition
        ))
    ));
    state = example();
    state.progress.next_marker_id = state.progress.markers[0].id;
    assert!(matches!(
        state.validate(),
        Err(ResumeError::Progress(CheckpointError::InvalidNextMarkerId))
    ));
}

#[test]
fn score_must_equal_ten_times_the_collected_count_on_every_entry() {
    let mut state = example();
    state.progress.score = 10;
    for result in [
        state.validate(),
        encode_resume(&state).map(|_| ()),
        decode_resume(&wire_bytes(&state)).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(ResumeError::ScoreMismatch {
                score: 10,
                collected: 2
            })
        ));
    }
    state.progress.collected.clear();
    state.progress.score = 0;
    assert_eq!(
        decode_resume(&encode_resume(&state).unwrap()).unwrap(),
        state
    );
}

#[test]
fn player_translation_accepts_the_game_boundary_and_refuses_each_outside_axis() {
    for translation in [[-280.0, -150.0], [280.0, 150.0]] {
        let mut state = example();
        state.player.translation = translation;
        assert_eq!(
            decode_resume(&encode_resume(&state).unwrap()).unwrap(),
            state
        );
    }
    for (axis, value) in [(0, -280.01), (0, 280.01), (1, -150.01), (1, 150.01)] {
        let mut state = example();
        state.player.translation[axis] = value;
        for result in [
            encode_resume(&state).map(|_| ()),
            decode_resume(&wire_bytes(&state)).map(|_| ()),
        ] {
            assert!(matches!(
                result,
                Err(ResumeError::TranslationOutOfRange { axis: failed, .. }) if failed == axis
            ));
        }
    }
}

#[test]
fn nonfinite_transform_values_are_refused_before_json_can_replace_them_with_null() {
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..2 {
            let mut state = example();
            state.player.translation[axis] = value;
            assert!(matches!(
                encode_resume(&state),
                Err(ResumeError::NonFiniteTranslation { axis: failed }) if failed == axis
            ));
            state = example();
            state.player.scale[axis] = value;
            assert!(matches!(
                encode_resume(&state),
                Err(ResumeError::NonFiniteScale { axis: failed }) if failed == axis
            ));
        }
        let mut state = example();
        state.player.rotation = value;
        assert!(matches!(
            encode_resume(&state),
            Err(ResumeError::NonFiniteRotation)
        ));
    }
}

#[test]
fn finite_rotation_and_scale_keep_the_full_f32_capacity_including_signed_zero() {
    for value in [
        f32::MAX,
        -f32::MAX,
        f32::MIN_POSITIVE,
        f32::from_bits(1),
        0.0,
        -0.0,
    ] {
        let mut state = example();
        state.player.rotation = value;
        state.player.scale = [value, -value];
        let restored = decode_resume(&encode_resume(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.player.rotation.to_bits(), value.to_bits());
        assert_eq!(
            restored.player.scale.map(f32::to_bits),
            [value, -value].map(f32::to_bits)
        );
    }
}

#[test]
fn clock_timestep_is_nonzero_and_at_most_one_second() {
    let mut uninitialized = example();
    uninitialized.clock = GameClock::default();
    assert!(matches!(
        uninitialized.validate(),
        Err(ResumeError::InvalidTimestep(0))
    ));

    for timestep_nanos in [0, MAX_TIMESTEP_NANOS + 1] {
        let mut state = example();
        state.clock = GameClock {
            ticks: 0,
            elapsed_nanos: 0,
            timestep_nanos,
        };
        assert!(matches!(
            encode_resume(&state),
            Err(ResumeError::InvalidTimestep(actual)) if actual == timestep_nanos
        ));
        assert!(matches!(
            decode_resume(&wire_bytes(&state)),
            Err(ResumeError::InvalidTimestep(actual)) if actual == timestep_nanos
        ));
    }
    for timestep_nanos in [1, MAX_TIMESTEP_NANOS] {
        let mut state = example();
        state.clock = GameClock {
            ticks: 0,
            elapsed_nanos: 0,
            timestep_nanos,
        };
        assert_eq!(
            decode_resume(&encode_resume(&state).unwrap()).unwrap(),
            state
        );
        state.clock.ticks = 1;
        state.clock.elapsed_nanos = timestep_nanos;
        assert_eq!(
            decode_resume(&encode_resume(&state).unwrap()).unwrap(),
            state
        );
    }
}

#[test]
fn clock_rejects_inconsistency_overflow_and_exhaustion_without_truncating_large_ticks() {
    let mut state = example();
    state.clock.elapsed_nanos += 1;
    assert!(matches!(
        state.validate(),
        Err(ResumeError::ClockMismatch {
            expected: 300_000_000,
            actual: 300_000_001
        })
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::ClockMismatch { .. })
    ));

    state.clock = GameClock {
        ticks: u64::MAX / 2 + 1,
        elapsed_nanos: 0,
        timestep_nanos: 2,
    };
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::ClockOverflow)
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::ClockOverflow)
    ));
    state.clock = GameClock {
        ticks: u64::MAX,
        elapsed_nanos: u64::MAX,
        timestep_nanos: 1,
    };
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::ClockExhausted)
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::ClockExhausted)
    ));

    state.clock = GameClock {
        ticks: u64::MAX - 1,
        elapsed_nanos: u64::MAX - 1,
        timestep_nanos: 1,
    };
    assert_eq!(
        decode_resume(&encode_resume(&state).unwrap()).unwrap(),
        state
    );
}

#[test]
fn semantic_intent_and_counters_round_trip_without_runtime_identity_or_extra_caps() {
    assert_eq!(
        SessionCounters::default(),
        SessionCounters {
            travel_count: 0,
            restarts: 0,
            saves: 0,
            restores: 0,
        }
    );
    let mut state = example();
    state.counters = SessionCounters {
        travel_count: u32::MAX,
        restarts: u32::MAX,
        saves: u32::MAX,
        restores: u32::MAX,
    };
    for held in [[false; 4], [true; 4], [false, true, true, false]] {
        state.held = held;
        assert_eq!(
            decode_resume(&encode_resume(&state).unwrap()).unwrap(),
            state
        );
    }
}

#[test]
fn compatibility_has_a_nonempty_finite_unique_catalog_and_one_manifest() {
    let mut state = example();
    state.compatibility.levels.clear();
    assert!(matches!(
        state.validate(),
        Err(ResumeError::InvalidContentRecordCount(0))
    ));
    state = example();
    state
        .compatibility
        .levels
        .push(state.compatibility.levels[0]);
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::InvalidContentRecordCount(3))
    ));
    state = example();
    state.compatibility.levels[1] = state.compatibility.levels[0];
    assert!(matches!(
        state.validate(),
        Err(ResumeError::DuplicateLevel(LevelId::Meadow))
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::DuplicateLevel(LevelId::Meadow))
    ));
    state = example();
    state.compatibility.levels[1].manifest[31] ^= 1;
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::ConflictingManifest(LevelId::Cavern))
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::ConflictingManifest(LevelId::Cavern))
    ));
}

#[test]
fn required_content_includes_startup_active_collected_and_marker_levels() {
    let mut state = example();
    state.compatibility.levels.remove(0);
    assert!(matches!(
        state.validate(),
        Err(ResumeError::MissingLevel(LevelId::Meadow))
    ));

    state = example();
    state.compatibility.levels.pop();
    assert!(matches!(
        state.validate(),
        Err(ResumeError::MissingLevel(LevelId::Cavern))
    ));
    state.progress.location.level = LevelKey::main(LevelId::Meadow);
    for marker in &mut state.progress.markers {
        marker.level = LevelKey::main(LevelId::Meadow);
    }
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::MissingLevel(LevelId::Cavern))
    ));
    state.progress.collected[1].level = LevelKey::main(LevelId::Meadow);
    state.progress.markers[1].level = LevelKey::new(LevelId::Cavern, 2).unwrap();
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::MissingLevel(LevelId::Cavern))
    ));
    state.progress.markers[1].level = LevelKey::main(LevelId::Meadow);
    state.progress.collected.sort();
    assert_eq!(
        decode_resume(&encode_resume(&state).unwrap()).unwrap(),
        state
    );
}

#[test]
fn configuration_is_required_and_independent_of_the_schema_digest() {
    let state = example();
    let mut changed = state.clone();
    changed.compatibility.configuration[31] ^= 1;
    assert_eq!(changed.compatibility.schema, state.compatibility.schema);
    assert_ne!(
        encode_resume(&changed).unwrap(),
        encode_resume(&state).unwrap()
    );
    assert_eq!(
        decode_resume(&encode_resume(&changed).unwrap()).unwrap(),
        changed
    );

    let mut wire = example_wire();
    wire["payload"]["compatibility"]
        .as_object_mut()
        .unwrap()
        .remove("configuration");
    assert!(matches!(decode_value(&wire), Err(ResumeError::Json(_))));
}

#[test]
fn hashes_preserve_all_32_bytes_and_zero_patterns_do_not_claim_cryptographic_admission() {
    let mut state = example();
    state.compatibility.schema = [0; 32];
    state.compatibility.configuration = [0; 32];
    for stamp in &mut state.compatibility.levels {
        stamp.manifest = [0; 32];
        stamp.content = [0; 32];
    }
    let zero_bytes = encode_resume(&state).unwrap();
    assert_eq!(decode_resume(&zero_bytes).unwrap(), state);

    state.compatibility.schema[31] = 255;
    state.compatibility.configuration[31] = 251;
    for stamp in &mut state.compatibility.levels {
        stamp.manifest[31] = 254;
        stamp.content[31] = 253;
    }
    let bytes = encode_resume(&state).unwrap();
    assert_ne!(bytes, zero_bytes);
    assert_eq!(decode_resume(&bytes).unwrap(), state);
}

#[test]
fn maximum_progress_capacity_fits_with_full_content_stamps_and_identifier_lengths() {
    let mut state = example();
    state.progress.location.anchor = SceneEntityId::new("a".repeat(MAX_OBJECT_ID_BYTES)).unwrap();
    state.progress.collected = (0..MAX_COLLECTED_PICKUPS)
        .map(|index| CollectedPickup {
            level: LevelKey::main(LevelId::Meadow),
            entity: SceneEntityId::new(format!(
                "{index:03}{}",
                "a".repeat(MAX_OBJECT_ID_BYTES - 3)
            ))
            .unwrap(),
        })
        .collect();
    state.progress.score = MAX_COLLECTED_PICKUPS as u32 * 10;
    state.progress.markers = (1..=MAX_MARKERS as u64)
        .map(|id| MarkerRecord {
            id: MarkerId::new(id).unwrap(),
            level: state.progress.location.level,
            position: [-MAX_MARKER_COORDINATE, MAX_MARKER_COORDINATE],
        })
        .collect();
    state.progress.next_marker_id = MarkerId::new(MAX_MARKERS as u64 + 1).unwrap();
    let bytes = encode_resume(&state).unwrap();
    assert!(bytes.len() < MAX_RESUME_BYTES);
    assert_eq!(decode_resume(&bytes).unwrap(), state);

    state.progress.collected.push(CollectedPickup {
        level: LevelKey::main(LevelId::Cavern),
        entity: SceneEntityId::new("extra").unwrap(),
    });
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::Progress(
            CheckpointError::TooManyCollectedPickups
        ))
    ));
    state.progress.collected.pop();
    state.progress.markers.push(MarkerRecord {
        id: state.progress.next_marker_id,
        level: state.progress.location.level,
        position: [0.0; 2],
    });
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::Progress(CheckpointError::TooManyMarkers))
    ));
}

#[test]
fn incompatible_headers_have_explicit_errors_before_payload_decoding() {
    let mut wire = example_wire();
    wire["payload"] = Value::Null;
    wire["kind"] = json!("scene");
    assert!(matches!(decode_value(&wire), Err(ResumeError::WrongKind)));
    wire["kind"] = json!(KIND);
    wire["format_version"] = json!(2);
    assert!(matches!(
        decode_value(&wire),
        Err(ResumeError::UnsupportedVersion(2))
    ));
    wire["format_version"] = json!(CURRENT_FORMAT_VERSION);
    wire["game_compatibility"] = json!(2);
    assert!(matches!(
        decode_value(&wire),
        Err(ResumeError::UnsupportedGameCompatibility(2))
    ));
}

#[test]
fn unknown_fields_are_refused_at_every_record_boundary() {
    for path in [
        "",
        "/payload",
        "/payload/progress",
        "/payload/progress/location",
        "/payload/progress/location/level",
        "/payload/progress/collected/0",
        "/payload/progress/collected/0/level",
        "/payload/progress/markers/0",
        "/payload/progress/markers/0/level",
        "/payload/player",
        "/payload/clock",
        "/payload/counters",
        "/payload/compatibility",
        "/payload/compatibility/levels/0",
    ] {
        let mut wire = example_wire();
        wire.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            matches!(decode_value(&wire), Err(ResumeError::Json(_))),
            "{path}"
        );
    }
}

#[test]
fn complete_records_and_fixed_arrays_cannot_be_silently_defaulted_or_truncated() {
    for field in [
        "progress",
        "player",
        "held",
        "clock",
        "counters",
        "compatibility",
    ] {
        let mut wire = example_wire();
        wire["payload"].as_object_mut().unwrap().remove(field);
        assert!(
            matches!(decode_value(&wire), Err(ResumeError::Json(_))),
            "{field}"
        );
    }
    for (path, value) in [
        ("/payload/held", json!([true, false, true])),
        ("/payload/held", json!(vec![false; 5])),
        ("/payload/player/translation", json!([0.0])),
        ("/payload/player/scale", json!([1.0, 1.0, 1.0])),
        ("/payload/compatibility/schema", json!(vec![0; 31])),
        ("/payload/compatibility/schema", json!(vec![0; 33])),
        ("/payload/compatibility/configuration", json!(vec![0; 31])),
        ("/payload/compatibility/configuration", json!(vec![0; 33])),
        (
            "/payload/compatibility/levels/0/manifest",
            json!(vec![0; 31]),
        ),
        (
            "/payload/compatibility/levels/0/content",
            json!(vec![0; 33]),
        ),
    ] {
        let mut wire = example_wire();
        *wire.pointer_mut(path).unwrap() = value;
        assert!(
            matches!(decode_value(&wire), Err(ResumeError::Json(_))),
            "{path}"
        );
    }
}

#[test]
fn byte_and_depth_preflight_run_before_header_or_payload_decoding() {
    assert!(matches!(
        decode_resume(&vec![b' '; MAX_RESUME_BYTES + 1]),
        Err(ResumeError::ByteLimitExceeded)
    ));
    let mut padded = encode_resume(&example()).unwrap();
    padded.resize(MAX_RESUME_BYTES, b' ');
    assert_eq!(decode_resume(&padded).unwrap(), example());
    padded.push(b' ');
    assert!(matches!(
        decode_resume(&padded),
        Err(ResumeError::ByteLimitExceeded)
    ));

    let deep = format!("{}0{}", "[".repeat(20), "]".repeat(20));
    assert!(matches!(
        decode_resume(deep.as_bytes()),
        Err(ResumeError::Shape(SerdeShapeError::DepthExceeded {
            maximum: 16
        }))
    ));
}

#[test]
fn structural_preflight_bounds_nodes_containers_and_strings_and_refuses_duplicate_keys() {
    let wide = serde_json::to_vec(&vec![0; 257]).unwrap();
    assert!(matches!(
        decode_resume(&wide),
        Err(ResumeError::Shape(
            SerdeShapeError::ContainerItemLimitExceeded { maximum: 256 }
        ))
    ));
    let row = serde_json::to_string(&vec![0; 256]).unwrap();
    let nodes = format!("[{}]", vec![row; 16].join(","));
    assert!(matches!(
        decode_resume(nodes.as_bytes()),
        Err(ResumeError::Shape(SerdeShapeError::NodeLimitExceeded {
            maximum: 4096
        }))
    ));
    let mut state = example();
    state.progress.location.anchor =
        SceneEntityId::new("a".repeat(MAX_OBJECT_ID_BYTES + 1)).unwrap();
    assert!(matches!(
        encode_resume(&state),
        Err(ResumeError::Progress(CheckpointError::ObjectIdTooLong))
    ));
    assert!(matches!(
        decode_resume(&wire_bytes(&state)),
        Err(ResumeError::Shape(SerdeShapeError::StringLimitExceeded {
            maximum: 128
        }))
    ));
    let row = vec!["a".repeat(MAX_OBJECT_ID_BYTES); 129];
    let strings = serde_json::to_vec(&[&row, &row]).unwrap();
    assert!(matches!(
        decode_resume(&strings),
        Err(ResumeError::Shape(
            SerdeShapeError::TotalStringLimitExceeded { maximum: 32768 }
        ))
    ));
    let bytes = String::from_utf8(encode_resume(&example()).unwrap()).unwrap();
    let duplicate = bytes.replace("\"ticks\":12", "\"ticks\":12,\"ticks\":13");
    assert_ne!(duplicate, bytes);
    assert!(matches!(
        decode_resume(duplicate.as_bytes()),
        Err(ResumeError::Shape(SerdeShapeError::DuplicateMapKey))
    ));
}

#[test]
fn malformed_json_and_trailing_values_are_not_admitted() {
    for bytes in [b"".as_slice(), b"{", b"null", b"[]"] {
        assert!(matches!(decode_resume(bytes), Err(ResumeError::Json(_))));
    }
    let mut bytes = encode_resume(&example()).unwrap();
    bytes.extend_from_slice(b" null");
    assert!(matches!(decode_resume(&bytes), Err(ResumeError::Json(_))));
    let mut wire = example_wire();
    wire["payload"]["player"]["rotation"] = json!(1e100);
    assert!(decode_value(&wire).is_err());
}

#[test]
fn checkpoint_persistence_and_version_one_migration_keep_their_own_score_semantics() {
    let mut progress = example().progress;
    progress.score = 7;
    let bytes = encode_checkpoint(&progress).unwrap();
    let restored = decode_checkpoint(&bytes).unwrap();
    assert_eq!(restored.migrated_from, None);
    assert_eq!(restored.checkpoint, progress);
    assert!(decode_resume(&bytes).is_err());
    assert!(decode_checkpoint(&encode_resume(&example()).unwrap()).is_err());

    let legacy = br#"{
        "kind":"two_level_checkpoint", "format_version":1, "campaign_revision":1,
        "payload":{
            "location":{"level":"cavern","anchor":"checkpoint"},
            "score":7,
            "collected":[{"level":"meadow","entity":"gem-near"}],
            "markers":[{"id":1,"level":"cavern","position":[40.0,12.5]}],
            "next_marker_id":2
        }
    }"#;
    let migrated = decode_checkpoint(legacy).unwrap();
    assert_eq!(migrated.migrated_from, Some(1));
    assert_eq!(migrated.checkpoint.score, 7);
    assert_eq!(
        migrated.checkpoint.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(
        migrated.checkpoint.collected[0].level,
        LevelKey::main(LevelId::Meadow)
    );
    assert_eq!(
        migrated.checkpoint.markers[0].level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(migrated.checkpoint.next_marker_id.get(), 2);
    let current = encode_checkpoint(&migrated.checkpoint).unwrap();
    let decoded = decode_checkpoint(&current).unwrap();
    assert_eq!(decoded.migrated_from, None);
    assert_eq!(decoded.checkpoint, migrated.checkpoint);
}

fn example() -> ResumeState {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::new(LevelId::Cavern, 2).unwrap();
    ResumeState {
        progress: Checkpoint {
            location: CheckpointLocation {
                level: cavern,
                anchor: SceneEntityId::new("checkpoint").unwrap(),
            },
            score: 20,
            collected: vec![
                CollectedPickup {
                    level: meadow,
                    entity: SceneEntityId::new("gem-near").unwrap(),
                },
                CollectedPickup {
                    level: cavern,
                    entity: SceneEntityId::new("gem-away").unwrap(),
                },
            ],
            markers: vec![
                MarkerRecord {
                    id: MarkerId::new(1).unwrap(),
                    level: meadow,
                    position: [-80.0, 0.0],
                },
                MarkerRecord {
                    id: MarkerId::new(2).unwrap(),
                    level: cavern,
                    position: [40.0, 12.5],
                },
            ],
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
        counters: SessionCounters {
            travel_count: 3,
            restarts: 2,
            saves: 4,
            restores: 1,
        },
        compatibility: ResumeCompatibility {
            schema: [9; 32],
            configuration: [10; 32],
            levels: vec![
                LevelContentStamp {
                    level: LevelId::Meadow,
                    manifest: [4; 32],
                    content: [5; 32],
                },
                LevelContentStamp {
                    level: LevelId::Cavern,
                    manifest: [4; 32],
                    content: [7; 32],
                },
            ],
        },
    }
}

fn wire_bytes(state: &ResumeState) -> Vec<u8> {
    serde_json::to_vec(&Envelope {
        kind: KIND,
        format_version: CURRENT_FORMAT_VERSION,
        game_compatibility: GAME_COMPATIBILITY,
        payload: state,
    })
    .unwrap()
}

fn example_wire() -> Value {
    serde_json::from_slice(&wire_bytes(&example())).unwrap()
}

fn decode_value(wire: &Value) -> Result<ResumeState, ResumeError> {
    decode_resume(&serde_json::to_vec(wire).unwrap())
}
