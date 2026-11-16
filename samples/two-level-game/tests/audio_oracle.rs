#![cfg(all(windows, feature = "audio-cpal"))]

#[path = "../src/bin/audio_desktop/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;
use two_level_game::{
    audio::{AudioCause, AudioClip, AudioPhase, AudioRecord, AudioSnapshot},
    levels::{LevelId, LevelKey},
};

fn fixture() -> (
    AudioSnapshot,
    Vec<f32>,
    Vec<AudioCause>,
    BTreeMap<u64, LevelKey>,
) {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let cause = AudioCause::Pickup {
        level: meadow,
        entity: "gem-near".into(),
    };
    let cue = |phase, frame| AudioRecord::Cue {
        runtime: 7,
        instance: 1,
        sequence: 1,
        clip: AudioClip::Pickup,
        cause: cause.clone(),
        frame,
        phase,
    };
    let records = vec![
        AudioRecord::ScopeApplied {
            runtime: 7,
            instance: 1,
            frame: 0,
        },
        cue(AudioPhase::Accepted, 0),
        cue(AudioPhase::Started, 0),
        cue(AudioPhase::Completed, 5760),
        AudioRecord::ScopeApplied {
            runtime: 7,
            instance: 2,
            frame: 5760,
        },
        AudioRecord::ScopeApplied {
            runtime: 7,
            instance: 3,
            frame: 5761,
        },
    ];
    let bytes = include_bytes!("../assets/audio/pickup.wav");
    let mut output = bytes[44..]
        .chunks_exact(2)
        .flat_map(|bytes| [f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0; 2])
        .collect::<Vec<_>>();
    output.extend([0.0; 4]);
    let snapshot = AudioSnapshot {
        runtime: 7,
        desired_instance: 3,
        applied_instance: 3,
        play_admitted: true,
        callbacks: 8,
        output_frames: 5762,
        accepted: 1,
        started: 1,
        completed: 1,
        cancelled: 0,
        faults: 0,
        admission_open: false,
        stream_drop_returned: true,
        data_callback_released: true,
        error_callback_released: true,
        physical_complete: true,
        error: None,
        records,
    };
    (
        snapshot,
        output,
        vec![cause],
        BTreeMap::from([(1, meadow), (2, cavern), (3, meadow)]),
    )
}

#[test]
fn independent_callback_oracle_accepts_the_literal_wav_and_refuses_one_sample_corruption() {
    let (snapshot, mut output, causes, instances) = fixture();
    let proof = oracle::verify(&snapshot, &output, &causes, &instances, false).unwrap();
    assert_eq!(proof.maximum_absolute_error, 0.0);
    output[300] += 0.01;
    assert!(oracle::verify(&snapshot, &output, &causes, &instances, false).is_err());
}

#[test]
fn a_matching_completed_wav_cannot_outlive_its_first_scene_replacement() {
    let (mut snapshot, output, causes, instances) = fixture();
    snapshot.records[4] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 2,
        frame: 100,
    };
    snapshot.records[5] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 3,
        frame: 300,
    };
    assert!(oracle::verify(&snapshot, &output, &causes, &instances, false).is_err());
}

#[test]
fn cancellation_at_a_second_replacement_cannot_hide_output_after_the_first_cut() {
    let (mut snapshot, mut output, causes, instances) = fixture();
    let mut cancelled = snapshot.records.remove(3);
    if let AudioRecord::Cue { phase, frame, .. } = &mut cancelled {
        *phase = AudioPhase::Cancelled { started: true };
        *frame = 300;
    }
    snapshot.records[3] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 2,
        frame: 100,
    };
    snapshot.records[4] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 3,
        frame: 300,
    };
    snapshot.records.insert(4, cancelled);
    snapshot.completed = 0;
    snapshot.cancelled = 1;
    output[600..].fill(0.0);
    assert!(oracle::verify(&snapshot, &output, &causes, &instances, true).is_err());
}

#[test]
fn a_scope_without_a_new_cue_still_requires_an_actual_game_instance() {
    let (mut snapshot, output, causes, instances) = fixture();
    snapshot.records[4] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 999,
        frame: 5760,
    };
    snapshot.records[5] = AudioRecord::ScopeApplied {
        runtime: 7,
        instance: 1000,
        frame: 5761,
    };
    snapshot.desired_instance = 1000;
    snapshot.applied_instance = 1000;
    assert!(oracle::verify(&snapshot, &output, &causes, &instances, false).is_err());
}
