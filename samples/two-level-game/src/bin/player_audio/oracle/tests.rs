use super::*;
use two_level_game::audio::AudioSnapshot;

const PICKUP: [f32; 2] = [0.25, -0.5];
const TRAVEL: [f32; 3] = [0.125, 0.25, -0.125];

fn fixture() -> (AudioObservation, Vec<f32>, BTreeMap<u64, LevelKey>) {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let other = LevelKey::new(LevelId::Cavern, 1).unwrap();
    let instances = BTreeMap::from([
        (1, meadow),
        (2, cavern),
        (3, other),
        (4, cavern),
        (5, meadow),
        (6, meadow),
    ]);
    let trajectory = [
        (
            1,
            AudioCause::Pickup {
                level: meadow,
                entity: "gem-near".into(),
            },
            0,
            2,
        ),
        (2, AudioCause::Travel { level: cavern }, 2, 3),
        (
            2,
            AudioCause::Pickup {
                level: cavern,
                entity: "gem-near".into(),
            },
            5,
            2,
        ),
        (3, AudioCause::Travel { level: other }, 7, 3),
        (
            3,
            AudioCause::Pickup {
                level: other,
                entity: "gem-near".into(),
            },
            10,
            2,
        ),
        (4, AudioCause::Travel { level: cavern }, 12, 3),
        (5, AudioCause::Travel { level: meadow }, 15, 3),
    ];
    let mut records = Vec::new();
    let mut current = 0;
    for (index, (instance, cause, start, frames)) in trajectory.into_iter().enumerate() {
        if instance != current {
            records.push(AudioRecord::ScopeApplied {
                runtime: 7,
                instance,
                frame: start,
            });
            current = instance;
        }
        let source = match cause {
            AudioCause::Pickup { .. } => AudioClip::Pickup,
            AudioCause::Travel { .. } => AudioClip::Travel,
        };
        for (phase, frame) in [
            (AudioPhase::Accepted, start),
            (AudioPhase::Started, start),
            (AudioPhase::Completed, start + frames),
        ] {
            records.push(AudioRecord::Cue {
                runtime: 7,
                instance,
                sequence: index as u64 + 1,
                clip: source,
                cause: cause.clone(),
                frame,
                phase,
            });
        }
    }
    records.push(AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 6,
        frame: 18,
    });
    let actual = [
        0.25, -0.5, 0.125, 0.25, -0.125, 0.25, -0.5, 0.125, 0.25, -0.125, 0.25, -0.5, 0.125, 0.25,
        -0.125, 0.125, 0.25, -0.125, 0.0, 0.0,
    ]
    .into_iter()
    .flat_map(|sample| [sample; CHANNELS])
    .collect();
    (
        AudioObservation {
            snapshot: AudioSnapshot {
                runtime: 7,
                reset_on_rebuild: false,
                output_enabled: false,
                suppressed_effects: 0,
                desired_instance: 6,
                applied_instance: 6,
                play_admitted: true,
                callbacks: 4,
                output_frames: 20,
                accepted: 7,
                started: 7,
                completed: 7,
                cancelled: 0,
                faults: 0,
                admission_open: false,
                stream_drop_returned: true,
                data_callback_released: true,
                error_callback_released: true,
                physical_complete: true,
                error: None,
                records,
            },
            evicted_records: 0,
        },
        actual,
        instances,
    )
}

#[test]
fn verification_requires_exact_callback_ordinals_across_drains() {
    let chunk = |first_frame| AudioOutput {
        first_frame,
        first_channel: 0,
        channels: CHANNELS as u16,
        samples: vec![0.25; CHANNELS],
    };
    let mut output = Vec::new();
    append_output(&mut output, vec![chunk(0)], 0).unwrap();
    for frame in [0, 2] {
        assert_eq!(
            append_output(&mut output, vec![chunk(frame)], 0).unwrap_err(),
            "cooked audio callback capture is unexpected or discontinuous"
        );
    }
    assert_eq!(output, [0.25; CHANNELS]);
    append_output(&mut output, vec![chunk(1)], 0).unwrap();
    assert_eq!(output, [0.25; CHANNELS * 2]);
}

#[test]
fn verification_refuses_reported_loss_without_waiting_for_a_gap() {
    let mut output = vec![0.25; CHANNELS];
    assert_eq!(
        append_output(&mut output, Vec::new(), 1).unwrap_err(),
        "cooked audio callback capture is incomplete (dropped frames = 1)"
    );
    assert_eq!(output, [0.25; CHANNELS]);
}

#[test]
fn compares_every_callback_sample_and_reports_exact_acquired_source_frames() {
    let (observation, actual, instances) = fixture();
    let proof = verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).unwrap();
    assert_eq!(proof["samples_compared"], 40);
    assert_eq!(proof["pickup_source_frames"], 2);
    assert_eq!(proof["travel_source_frames"], 3);
    assert_eq!(proof["accepted_pickups"], 3);
    assert_eq!(proof["accepted_travels"], 4);
    assert_eq!(proof["maximum_absolute_error"], 0.0);
    assert_ne!(proof["pickup_source_blake3"], proof["travel_source_blake3"]);
}

#[test]
fn refuses_reported_history_eviction_even_when_samples_and_counters_match() {
    let (mut observation, actual, instances) = fixture();
    observation.evicted_records = 1;
    assert_eq!(
        verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).unwrap_err(),
        "cooked audio diagnostic history is incomplete (evicted records = 1)"
    );
}

#[test]
fn rejects_a_corrupt_callback_sample_or_wrong_acquired_pcm() {
    let (observation, mut actual, instances) = fixture();
    actual[0] = 0.0;
    assert!(verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).is_err());
    let (observation, actual, instances) = fixture();
    assert!(verify(&observation, &actual, &[0.5, -0.5], &TRAVEL, &instances).is_err());
}

#[test]
fn refuses_pcm_acceptance_with_a_missing_final_frame() {
    let (observation, mut actual, instances) = fixture();
    actual.truncate(actual.len() - CHANNELS);
    assert_eq!(
        verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).unwrap_err(),
        "callback PCM does not cover the complete bounded output interval"
    );
}

#[test]
fn refuses_pcm_acceptance_before_physical_callback_release() {
    let (mut observation, actual, instances) = fixture();
    observation.snapshot.error_callback_released = false;
    assert!(verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).is_err());
}

#[test]
fn rejects_a_cue_from_another_scene_instance() {
    let (mut observation, actual, instances) = fixture();
    for record in &mut observation.snapshot.records {
        if let AudioRecord::Cue {
            instance,
            sequence: 1,
            ..
        } = record
        {
            *instance = 2;
        }
    }
    assert!(verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).is_err());
}

#[test]
fn cancellation_requires_the_exact_scene_replacement_output_boundary() {
    let (mut observation, mut actual, instances) = fixture();
    observation.snapshot.completed = 6;
    observation.snapshot.cancelled = 1;
    for record in &mut observation.snapshot.records {
        match record {
            AudioRecord::Cue {
                sequence: 1,
                phase,
                frame,
                ..
            } if matches!(phase, AudioPhase::Completed) => {
                *phase = AudioPhase::Cancelled { started: true };
                *frame = 1;
            }
            AudioRecord::ScopeApplied {
                instance: 2, frame, ..
            } => *frame = 1,
            _ => {}
        }
    }
    actual[2] = 0.0;
    actual[3] = 0.0;
    let proof = verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).unwrap();
    assert_eq!(proof["stale_scope_cancellations"], 1);
    for record in &mut observation.snapshot.records {
        if let AudioRecord::ScopeApplied {
            instance: 2, frame, ..
        } = record
        {
            *frame = 2;
        }
    }
    assert!(verify(&observation, &actual, &PICKUP, &TRAVEL, &instances).is_err());
}
