use nara::identity::SceneEntityId;
use serde_json::{Value, json};
use two_level_game::{
    levels::{LevelId, LevelKey, MAX_LEVEL_INSTANCES},
    save::{
        CURRENT_FORMAT_VERSION, Checkpoint, CheckpointError, CheckpointLocation, CollectedPickup,
        MAX_CHECKPOINT_BYTES, MAX_COLLECTED_PICKUPS, MAX_MARKER_COORDINATE, MAX_MARKERS,
        MAX_OBJECT_ID_BYTES, MAX_SCORE, MarkerId, MarkerRecord, decode_checkpoint,
        encode_checkpoint,
    },
};

#[test]
fn logical_instances_share_a_source_without_sharing_persistent_object_keys() {
    assert_eq!(LevelId::STARTUP, LevelId::Meadow);
    assert_eq!(LevelId::Meadow.scene_path().as_str(), "meadow.scene.json");
    assert_eq!(LevelId::Cavern.scene_path().as_str(), "cavern.scene.json");
    let first = LevelKey::main(LevelId::Meadow);
    let second = LevelKey::new(LevelId::Meadow, 1).unwrap();
    assert_ne!(first, second);
    assert_eq!(first.level().scene_path(), second.level().scene_path());
    assert_eq!(second.instance(), 1);
    assert!(LevelKey::new(LevelId::Cavern, MAX_LEVEL_INSTANCES).is_err());

    let mut checkpoint = example();
    checkpoint.collected = vec![pickup(first, "gem"), pickup(second, "gem")];
    let loaded = decode_checkpoint(&encode_checkpoint(&checkpoint).unwrap()).unwrap();
    assert_eq!(loaded.checkpoint.collected.len(), 2);
    assert_ne!(
        loaded.checkpoint.collected[0],
        loaded.checkpoint.collected[1]
    );
}

#[test]
fn current_checkpoint_round_trips_explicit_state_and_canonicalizes_set_order() {
    let mut checkpoint = example();
    checkpoint
        .collected
        .push(pickup(LevelKey::main(LevelId::Cavern), "gem-east"));
    checkpoint.markers.push(MarkerRecord {
        id: MarkerId::new(2).unwrap(),
        level: LevelKey::new(LevelId::Meadow, 1).unwrap(),
        position: [-12.0, 3.5],
    });
    checkpoint.next_marker_id = MarkerId::new(3).unwrap();
    let bytes = encode_checkpoint(&checkpoint).unwrap();
    let decoded = decode_checkpoint(&bytes).unwrap();
    assert_eq!(decoded.migrated_from, None);
    assert_eq!(decoded.checkpoint.location, checkpoint.location);
    assert_eq!(decoded.checkpoint.score, 10);
    assert_eq!(decoded.checkpoint.collected.len(), 2);
    assert_eq!(decoded.checkpoint.markers.len(), 2);
    assert_eq!(decoded.checkpoint.next_marker_id.get(), 3);
    assert_eq!(decoded.checkpoint.markers[1].position, [-12.0, 3.5]);

    checkpoint.collected.reverse();
    checkpoint.markers.reverse();
    assert_eq!(encode_checkpoint(&checkpoint).unwrap(), bytes);
    assert_eq!(encode_checkpoint(&decoded.checkpoint).unwrap(), bytes);
    let wire: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(wire["format_version"], CURRENT_FORMAT_VERSION);
    assert_eq!(wire["kind"], "two_level_checkpoint");
}

#[test]
fn version_one_migrates_source_keys_into_main_instances_without_losing_state() {
    let bytes = br#"{
        "kind":"two_level_checkpoint", "format_version":1, "campaign_revision":1,
        "payload":{
            "location":{"level":"cavern","anchor":"entry"},
            "score":10,
            "collected":[{"level":"meadow","entity":"gem-west"}],
            "markers":[{"id":1,"level":"cavern","position":[4.0,-2.0]}],
            "next_marker_id":2
        }
    }"#;
    let loaded = decode_checkpoint(bytes).unwrap();
    assert_eq!(loaded.migrated_from, Some(1));
    assert_eq!(loaded.checkpoint, example());
    let current = encode_checkpoint(&loaded.checkpoint).unwrap();
    let wire: Value = serde_json::from_slice(&current).unwrap();
    assert_eq!(wire["format_version"], CURRENT_FORMAT_VERSION);
    assert_eq!(wire["payload"]["location"]["level"]["instance"], 0);
    assert_eq!(decode_checkpoint(&current).unwrap().migrated_from, None);
}

#[test]
fn malformed_foreign_and_unsupported_files_are_not_admitted() {
    for bytes in [&b""[..], &b"{"[..], &b"null"[..], &b"[]"[..]] {
        assert!(decode_checkpoint(bytes).is_err());
    }
    let mut wire = example_wire();
    wire["kind"] = json!("scene");
    assert!(matches!(
        decode_value(&wire),
        Err(CheckpointError::WrongKind)
    ));
    wire = example_wire();
    wire["format_version"] = json!(99);
    assert!(matches!(
        decode_value(&wire),
        Err(CheckpointError::UnsupportedVersion(99))
    ));
    wire = example_wire();
    wire["campaign_revision"] = json!(2);
    assert!(matches!(
        decode_value(&wire),
        Err(CheckpointError::UnsupportedCampaign(2))
    ));
    let mut bytes = encode_checkpoint(&example()).unwrap();
    bytes.extend_from_slice(b" null");
    assert!(matches!(
        decode_checkpoint(&bytes),
        Err(CheckpointError::Json(_))
    ));
}

#[test]
fn unknown_fields_are_rejected_at_each_owned_record_boundary() {
    for path in [
        vec![],
        vec!["payload"],
        vec!["payload", "location"],
        vec!["payload", "location", "level"],
        vec!["payload", "collected", "0"],
        vec!["payload", "markers", "0"],
    ] {
        let mut wire = example_wire();
        let mut record = &mut wire;
        for segment in path {
            record = if let Ok(index) = segment.parse::<usize>() {
                &mut record[index]
            } else {
                &mut record[segment]
            };
        }
        record["unknown"] = json!(true);
        assert!(matches!(decode_value(&wire), Err(CheckpointError::Json(_))));
    }
}

#[test]
fn duplicate_json_keys_and_duplicate_persistent_objects_are_rejected() {
    let bytes = String::from_utf8(encode_checkpoint(&example()).unwrap()).unwrap();
    let duplicate_key = bytes.replace("\"score\":10", "\"score\":10,\"score\":11");
    assert_ne!(duplicate_key, bytes);
    assert!(matches!(
        decode_checkpoint(duplicate_key.as_bytes()),
        Err(CheckpointError::Shape(_))
    ));

    let mut checkpoint = example();
    checkpoint.collected.push(checkpoint.collected[0].clone());
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::DuplicateCollectedPickup)
    ));
    let mut wire = example_wire();
    let pickup = wire["payload"]["collected"][0].clone();
    wire["payload"]["collected"]
        .as_array_mut()
        .unwrap()
        .push(pickup);
    assert!(matches!(
        decode_value(&wire),
        Err(CheckpointError::DuplicateCollectedPickup)
    ));

    checkpoint = example();
    let mut marker = checkpoint.markers[0].clone();
    marker.level = LevelKey::main(LevelId::Meadow);
    checkpoint.markers.push(marker);
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::DuplicateMarkerId)
    ));
}

#[test]
fn coordinate_checks_reject_nonfinite_and_out_of_range_values_before_encoding() {
    for coordinate in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut checkpoint = example();
        checkpoint.markers[0].position[0] = coordinate;
        assert!(matches!(
            encode_checkpoint(&checkpoint),
            Err(CheckpointError::NonFiniteMarkerPosition)
        ));
    }
    for coordinate in [-MAX_MARKER_COORDINATE, MAX_MARKER_COORDINATE] {
        let mut checkpoint = example();
        checkpoint.markers[0].position = [coordinate; 2];
        decode_checkpoint(&encode_checkpoint(&checkpoint).unwrap()).unwrap();
    }
    let mut checkpoint = example();
    checkpoint.markers[0].position[1] = MAX_MARKER_COORDINATE + 1.0;
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::MarkerPositionOutOfRange)
    ));
    let mut wire = example_wire();
    wire["payload"]["markers"][0]["position"][0] = json!(1e100);
    assert!(decode_value(&wire).is_err());
}

#[test]
fn byte_shape_string_and_collection_limits_are_enforced() {
    assert!(matches!(
        decode_checkpoint(&vec![b' '; MAX_CHECKPOINT_BYTES + 1]),
        Err(CheckpointError::ByteLimitExceeded)
    ));
    let deep = format!("{}0{}", "[".repeat(20), "]".repeat(20));
    assert!(matches!(
        decode_checkpoint(deep.as_bytes()),
        Err(CheckpointError::Shape(_))
    ));
    let mut checkpoint = example();
    checkpoint.location.anchor = SceneEntityId::new("a".repeat(MAX_OBJECT_ID_BYTES + 1)).unwrap();
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::ObjectIdTooLong)
    ));
    let mut wire = example_wire();
    wire["payload"]["location"]["anchor"] = json!("a".repeat(MAX_OBJECT_ID_BYTES + 1));
    assert!(matches!(
        decode_value(&wire),
        Err(CheckpointError::Shape(_))
    ));

    checkpoint = example();
    checkpoint.collected = (0..MAX_COLLECTED_PICKUPS)
        .map(|index| pickup(LevelKey::main(LevelId::Meadow), &format!("gem-{index}")))
        .collect();
    checkpoint.markers = (1..=MAX_MARKERS as u64)
        .map(|id| MarkerRecord {
            id: MarkerId::new(id).unwrap(),
            level: LevelKey::main(LevelId::Cavern),
            position: [0.0, 0.0],
        })
        .collect();
    checkpoint.next_marker_id = MarkerId::new(MAX_MARKERS as u64 + 1).unwrap();
    let bytes = encode_checkpoint(&checkpoint).unwrap();
    assert!(bytes.len() < MAX_CHECKPOINT_BYTES);
    let loaded = decode_checkpoint(&bytes).unwrap();
    assert_eq!(loaded.checkpoint.collected.len(), MAX_COLLECTED_PICKUPS);
    assert_eq!(loaded.checkpoint.markers.len(), MAX_MARKERS);
    checkpoint
        .collected
        .push(pickup(LevelKey::main(LevelId::Cavern), "extra"));
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::TooManyCollectedPickups)
    ));
    checkpoint.collected.pop();
    checkpoint.markers.push(MarkerRecord {
        id: checkpoint.next_marker_id,
        level: LevelKey::main(LevelId::Cavern),
        position: [0.0, 0.0],
    });
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::TooManyMarkers)
    ));
}

#[test]
fn score_counter_and_logical_identifier_validation_reject_invalid_values() {
    let mut checkpoint = example();
    checkpoint.score = MAX_SCORE;
    decode_checkpoint(&encode_checkpoint(&checkpoint).unwrap()).unwrap();
    checkpoint.score += 1;
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::ScoreOutOfRange)
    ));
    checkpoint = example();
    checkpoint.next_marker_id = checkpoint.markers[0].id;
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::InvalidNextMarkerId)
    ));
    checkpoint.next_marker_id = MarkerId::new(u64::MAX).unwrap();
    assert!(matches!(
        encode_checkpoint(&checkpoint),
        Err(CheckpointError::InvalidNextMarkerId)
    ));
    assert_eq!(MarkerId::new(0), None);

    for invalid in [json!("unknown"), json!(0)] {
        let mut wire = example_wire();
        wire["payload"]["location"]["level"]["level"] = invalid;
        assert!(matches!(decode_value(&wire), Err(CheckpointError::Json(_))));
    }
    for instance in [u64::from(MAX_LEVEL_INSTANCES), 65536] {
        let mut wire = example_wire();
        wire["payload"]["location"]["level"]["instance"] = json!(instance);
        assert!(matches!(decode_value(&wire), Err(CheckpointError::Json(_))));
    }
    let mut wire = example_wire();
    wire["payload"]["location"]["anchor"] = json!("../entry");
    assert!(matches!(decode_value(&wire), Err(CheckpointError::Json(_))));
    wire = example_wire();
    wire["payload"]["markers"][0]["id"] = json!(0);
    assert!(matches!(decode_value(&wire), Err(CheckpointError::Json(_))));
}

fn example() -> Checkpoint {
    let mut checkpoint = Checkpoint::new(CheckpointLocation {
        level: LevelKey::main(LevelId::Cavern),
        anchor: SceneEntityId::new("entry").unwrap(),
    });
    checkpoint.score = 10;
    checkpoint
        .collected
        .push(pickup(LevelKey::main(LevelId::Meadow), "gem-west"));
    checkpoint.markers.push(MarkerRecord {
        id: MarkerId::new(1).unwrap(),
        level: LevelKey::main(LevelId::Cavern),
        position: [4.0, -2.0],
    });
    checkpoint.next_marker_id = MarkerId::new(2).unwrap();
    checkpoint
}

fn pickup(level: LevelKey, id: &str) -> CollectedPickup {
    CollectedPickup {
        level,
        entity: SceneEntityId::new(id).unwrap(),
    }
}

fn example_wire() -> Value {
    serde_json::from_slice(&encode_checkpoint(&example()).unwrap()).unwrap()
}

fn decode_value(value: &Value) -> Result<two_level_game::save::DecodedCheckpoint, CheckpointError> {
    decode_checkpoint(&serde_json::to_vec(value).unwrap())
}
