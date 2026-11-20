#![cfg(all(windows, feature = "audio-cpal"))]

#[path = "../src/bin/audio_desktop/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;
use two_level_game::{
    audio::{AudioCause, AudioClip, AudioObservation, AudioPhase, AudioRecord, AudioSnapshot},
    levels::{LevelId, LevelKey},
};

const SHIPPED: oracle::SourceWavs<'static> = oracle::SourceWavs {
    pickup: include_bytes!("../assets/audio/pickup.wav"),
    travel: include_bytes!("../assets/audio/travel.wav"),
};

fn fixture(
    pcm: &[u8],
) -> (
    AudioObservation,
    Vec<f32>,
    Vec<AudioCause>,
    BTreeMap<u64, LevelKey>,
) {
    let frames = (pcm.len() / 2) as u64;
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
        cue(AudioPhase::Completed, frames),
        AudioRecord::ScopeApplied {
            runtime: 7,
            instance: 2,
            frame: frames,
        },
        AudioRecord::ScopeApplied {
            runtime: 7,
            instance: 3,
            frame: frames + 1,
        },
    ];
    let mut output = pcm
        .chunks_exact(2)
        .flat_map(|bytes| [f32::from(i16::from_le_bytes(bytes.try_into().unwrap())) / 32768.0; 2])
        .collect::<Vec<_>>();
    output.extend([0.0; 4]);
    let snapshot = AudioSnapshot {
        runtime: 7,
        reset_on_rebuild: false,
        output_enabled: false,
        suppressed_effects: 0,
        desired_instance: 3,
        applied_instance: 3,
        play_admitted: true,
        callbacks: 8,
        output_frames: frames + 2,
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
        AudioObservation {
            snapshot,
            evicted_records: 0,
        },
        output,
        vec![cause],
        BTreeMap::from([(1, meadow), (2, cavern), (3, meadow)]),
    )
}

#[test]
fn independent_callback_oracle_accepts_the_captured_wav_and_locates_sample_corruption() {
    let (observation, mut output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    let proof = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap();
    assert_eq!(proof.maximum_absolute_error, 0.0);
    assert_eq!(
        (proof.pickup_source_frames, proof.travel_source_frames),
        (5760, 11520)
    );
    output[301] += 0.25;
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(
        error.contains("independent PCM mismatch at frame=150 channel=1"),
        "{error}"
    );
}

#[test]
fn a_matching_retained_history_with_an_evicted_prefix_cannot_qualify() {
    let (mut observation, output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    observation.evicted_records = 1;
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert_eq!(
        error,
        "audio diagnostic history is incomplete (evicted records = 1)"
    );
}

#[test]
fn independent_callback_oracle_uses_the_edited_pickup_amplitude() {
    let mut pickup = SHIPPED.pickup.to_vec();
    for sample in pickup[44..].chunks_exact_mut(2) {
        let original = i16::from_le_bytes([sample[0], sample[1]]);
        sample.copy_from_slice(&(original / 2).to_le_bytes());
    }
    let (observation, output, causes, instances) = fixture(&pickup[44..]);
    let sources = oracle::SourceWavs {
        pickup: &pickup,
        ..SHIPPED
    };
    let proof = oracle::verify(sources, &observation, &output, &causes, &instances).unwrap();
    assert_eq!(proof.pickup_source_frames, 5760);
    assert_eq!(proof.maximum_absolute_error, 0.0);
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(error.contains("independent PCM mismatch"), "{error}");
}

#[test]
fn independent_callback_oracle_uses_the_rebound_pickup_frame_boundary() {
    let (observation, output, causes, instances) = fixture(&SHIPPED.travel[44..]);
    let sources = oracle::SourceWavs {
        pickup: SHIPPED.travel,
        ..SHIPPED
    };
    let proof = oracle::verify(sources, &observation, &output, &causes, &instances).unwrap();
    assert_eq!(proof.pickup_source_frames, 11520);
    assert_eq!(proof.maximum_absolute_error, 0.0);
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(
        error.contains("source WAV boundary at frame=5760"),
        "{error}"
    );
}

#[test]
fn an_odd_ancillary_chunk_preserves_the_captured_pcm() {
    let (observation, output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    let mut pickup = SHIPPED.pickup.to_vec();
    pickup.splice(12..12, b"JUNK\x01\x00\x00\x00X\x00".iter().copied());
    let riff_length = (pickup.len() - 8) as u32;
    pickup[4..8].copy_from_slice(&riff_length.to_le_bytes());
    let sources = oracle::SourceWavs {
        pickup: &pickup,
        ..SHIPPED
    };
    let proof = oracle::verify(sources, &observation, &output, &causes, &instances).unwrap();
    assert_eq!(proof.maximum_absolute_error, 0.0);
}

#[test]
fn silent_authored_samples_are_compared_against_the_actual_silent_output() {
    let mut pickup = SHIPPED.pickup.to_vec();
    pickup[44..].fill(0);
    let (observation, output, causes, instances) = fixture(&pickup[44..]);
    let sources = oracle::SourceWavs {
        pickup: &pickup,
        ..SHIPPED
    };
    let proof = oracle::verify(sources, &observation, &output, &causes, &instances).unwrap();
    assert_eq!(proof.nonzero_samples, 0);
    assert_eq!(proof.maximum_absolute_error, 0.0);
}

#[test]
fn a_matching_completed_wav_cannot_outlive_its_first_scene_replacement() {
    let (mut observation, output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    let snapshot = &mut observation.snapshot;
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
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(error.contains("voice survived its first original Scene scope replacement"));
}

#[test]
fn cancellation_at_a_second_replacement_cannot_hide_output_after_the_first_cut() {
    let (mut observation, mut output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    let snapshot = &mut observation.snapshot;
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
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(error.contains("voice survived its first original Scene scope replacement"));
}

#[test]
fn a_scope_without_a_new_cue_still_requires_an_actual_game_instance() {
    let (mut observation, output, causes, instances) = fixture(&SHIPPED.pickup[44..]);
    let snapshot = &mut observation.snapshot;
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
    let error = oracle::verify(SHIPPED, &observation, &output, &causes, &instances).unwrap_err();
    assert!(error.contains("scope application has a foreign identity"));
}
