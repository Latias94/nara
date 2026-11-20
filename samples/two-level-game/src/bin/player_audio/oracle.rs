use std::collections::BTreeMap;

use nara::fs::ContentDigest;
use serde_json::{Value, json};
use two_level_game::{
    audio::{AudioCause, AudioClip, AudioObservation, AudioOutput, AudioPhase, AudioRecord},
    levels::{LevelId, LevelKey},
};

pub(super) const SAMPLE_RATE: u64 = 48_000;
pub(super) const CHANNELS: usize = 2;
const MAX_OUTPUT_FRAMES: u64 = SAMPLE_RATE * 60;

pub(super) fn append_output(
    output: &mut Vec<f32>,
    chunks: Vec<AudioOutput>,
    dropped_frames: u64,
) -> Result<(), String> {
    if dropped_frames != 0 {
        return Err(format!(
            "cooked audio callback capture is incomplete (dropped frames = {dropped_frames})"
        ));
    }
    for chunk in chunks {
        if chunk.channels as usize != CHANNELS
            || chunk.first_channel != 0
            || chunk.samples.is_empty()
            || !chunk.samples.len().is_multiple_of(CHANNELS)
            || chunk.first_frame != (output.len() / CHANNELS) as u64
        {
            return Err("cooked audio callback capture is unexpected or discontinuous".into());
        }
        if output.len() + chunk.samples.len() > MAX_OUTPUT_FRAMES as usize * CHANNELS {
            return Err("cooked audio callback capture exceeded its finite allowance".into());
        }
        output.extend(chunk.samples);
    }
    Ok(())
}

struct Cue {
    clip: AudioClip,
    cause: AudioCause,
    accepted: u64,
    start: Option<u64>,
    end: Option<(u64, bool)>,
}

fn expected_causes() -> Vec<AudioCause> {
    let meadow = LevelKey::main(LevelId::Meadow);
    let cavern = LevelKey::main(LevelId::Cavern);
    let other = LevelKey::new(LevelId::Cavern, 1).expect("fixed game level instance");
    vec![
        AudioCause::Pickup {
            level: meadow,
            entity: "gem-near".into(),
        },
        AudioCause::Travel { level: cavern },
        AudioCause::Pickup {
            level: cavern,
            entity: "gem-near".into(),
        },
        AudioCause::Travel { level: other },
        AudioCause::Pickup {
            level: other,
            entity: "gem-near".into(),
        },
        AudioCause::Travel { level: cavern },
        AudioCause::Travel { level: meadow },
    ]
}

fn clip(cause: &AudioCause) -> AudioClip {
    match cause {
        AudioCause::Pickup { .. } => AudioClip::Pickup,
        AudioCause::Travel { .. } => AudioClip::Travel,
    }
}

fn samples<'a>(cue: AudioClip, pickup: &'a [f32], travel: &'a [f32]) -> &'a [f32] {
    match cue {
        AudioClip::Pickup => pickup,
        AudioClip::Travel => travel,
    }
}

fn digest(samples: &[f32]) -> [u8; 32] {
    let bytes: Vec<_> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    *ContentDigest::of_bytes(&bytes).as_bytes()
}

pub(super) fn verify(
    observation: &AudioObservation,
    actual: &[f32],
    pickup: &[f32],
    travel: &[f32],
    instances: &BTreeMap<u64, LevelKey>,
) -> Result<Value, String> {
    let snapshot = &observation.snapshot;
    if !snapshot.play_admitted
        || !snapshot.physical_complete
        || !snapshot.stream_drop_returned
        || !snapshot.data_callback_released
        || !snapshot.error_callback_released
        || snapshot.admission_open
        || snapshot.error.is_some()
        || snapshot.faults != 0
    {
        return Err("PCM acceptance requires complete original stream/callback retirement".into());
    }
    if snapshot.callbacks == 0
        || snapshot.output_frames == 0
        || snapshot.output_frames > MAX_OUTPUT_FRAMES
        || actual.len() as u64 != snapshot.output_frames * CHANNELS as u64
    {
        return Err("callback PCM does not cover the complete bounded output interval".into());
    }
    if observation.evicted_records != 0 {
        return Err(format!(
            "cooked audio diagnostic history is incomplete (evicted records = {})",
            observation.evicted_records
        ));
    }
    for source in [pickup, travel] {
        if source.is_empty()
            || source.len() > SAMPLE_RATE as usize
            || source
                .iter()
                .any(|value| !value.is_finite() || !(-1.0..=1.0).contains(value))
            || source.iter().all(|value| *value == 0.0)
        {
            return Err("original acquired SoundBank contains an invalid or silent clip".into());
        }
    }
    let mut cues = BTreeMap::<(u64, u64), Cue>::new();
    let mut scopes = Vec::<(u64, u64)>::new();
    let mut causes = Vec::new();
    let mut starts = 0;
    let mut completions = 0;
    let mut cancellations = 0;
    let mut last_sequence = 0;
    let mut last_frame = 0;
    for record in &snapshot.records {
        let frame = match record {
            AudioRecord::ScopeApplied { frame, .. } | AudioRecord::Cue { frame, .. } => *frame,
        };
        if frame < last_frame || frame > snapshot.output_frames {
            return Err(
                "callback record output boundary regressed or exceeded complete capture".into(),
            );
        }
        last_frame = frame;
        match record {
            AudioRecord::ScopeApplied {
                runtime,
                instance,
                frame,
            } => {
                if *runtime != snapshot.runtime
                    || *instance == 0
                    || !instances.contains_key(instance)
                    || scopes.last().is_some_and(|&(prior, _)| prior >= *instance)
                {
                    return Err(
                        "callback scope does not belong to the original Scene trajectory".into(),
                    );
                }
                scopes.push((*instance, *frame));
            }
            AudioRecord::Cue {
                runtime,
                instance,
                sequence,
                clip: source,
                cause,
                frame,
                phase,
            } => {
                if *runtime != snapshot.runtime
                    || *instance == 0
                    || *sequence == 0
                    || *source != clip(cause)
                {
                    return Err("callback cue changed its Runtime or gameplay identity".into());
                }
                let key = (*instance, *sequence);
                if matches!(phase, AudioPhase::Accepted) {
                    let level = match cause {
                        AudioCause::Pickup { level, .. } | AudioCause::Travel { level } => level,
                    };
                    if instances.get(instance) != Some(level)
                        || scopes.last().map(|&(current, _)| current) != Some(*instance)
                        || *sequence <= last_sequence
                    {
                        return Err(
                            "accepted effect disagrees with the actual game Scene and event order"
                                .into(),
                        );
                    }
                    last_sequence = *sequence;
                    if cues
                        .insert(
                            key,
                            Cue {
                                clip: *source,
                                cause: cause.clone(),
                                accepted: *frame,
                                start: None,
                                end: None,
                            },
                        )
                        .is_some()
                    {
                        return Err("one gameplay cause was accepted more than once".into());
                    }
                    causes.push(cause.clone());
                    continue;
                }
                let cue = cues
                    .get_mut(&key)
                    .ok_or("callback cue preceded its acceptance")?;
                if cue.clip != *source || cue.cause != *cause || *frame < cue.accepted {
                    return Err("callback lifecycle changed its accepted gameplay cause".into());
                }
                match phase {
                    AudioPhase::Started => {
                        if scopes.last().map(|&(current, _)| current) != Some(*instance)
                            || cue.start.replace(*frame).is_some()
                            || cue.end.is_some()
                        {
                            return Err("one effect started twice or after scope retirement".into());
                        }
                        starts += 1;
                    }
                    AudioPhase::Completed => {
                        let start = cue.start.ok_or("unstarted effect reported completion")?;
                        if *frame != start + samples(*source, pickup, travel).len() as u64
                            || cue.end.replace((*frame, false)).is_some()
                        {
                            return Err(
                                "effect completion differs from its exact acquired PCM boundary"
                                    .into(),
                            );
                        }
                        completions += 1;
                    }
                    AudioPhase::Cancelled { started } => {
                        if *started != cue.start.is_some()
                            || cue.start.is_some_and(|start| *frame < start)
                            || cue.end.replace((*frame, true)).is_some()
                        {
                            return Err(
                                "effect cancellation conflicts with its callback lifecycle".into(),
                            );
                        }
                        cancellations += 1;
                    }
                    AudioPhase::Accepted | AudioPhase::Duplicate | AudioPhase::Refused { .. } => {
                        return Err(
                            "ordinary gameplay produced a duplicate or refused audio effect".into(),
                        );
                    }
                }
            }
        }
    }
    if causes != expected_causes()
        || snapshot.accepted != cues.len() as u64
        || snapshot.started != starts
        || snapshot.completed != completions
        || snapshot.cancelled != cancellations
        || completions + cancellations != snapshot.accepted
        || scopes.last().map(|&(instance, _)| instance) != Some(snapshot.applied_instance)
        || snapshot.desired_instance != snapshot.applied_instance
    {
        return Err(
            "callback causes, counters or final scope disagree with the complete Travel trajectory"
                .into(),
        );
    }
    let mut expected = vec![0.0_f32; snapshot.output_frames as usize];
    let mut stale_scope_cancellations = 0;
    for ((instance, _), cue) in cues {
        let (end, cancelled) = cue
            .end
            .ok_or("accepted effect has no callback terminal fact")?;
        let replacement = scopes
            .iter()
            .find(|&&(next, _)| next > instance)
            .map(|&(_, frame)| frame);
        if replacement.is_some_and(|cut| end > cut) || (cancelled && replacement != Some(end)) {
            return Err(
                "effect outlived its first Scene replacement or lacks its cancellation boundary"
                    .into(),
            );
        }
        stale_scope_cancellations += usize::from(cancelled);
        if let Some(start) = cue.start {
            let source = samples(cue.clip, pickup, travel);
            if end < start
                || end - start > source.len() as u64
                || (cancelled && end - start == source.len() as u64)
            {
                return Err(
                    "effect extends beyond its complete PCM or cancellation boundary".into(),
                );
            }
            for (offset, frame) in (start..end).enumerate() {
                expected[frame as usize] += source[offset];
            }
        }
    }
    let mut maximum_absolute_error = 0.0_f32;
    let mut nonzero_samples = 0;
    for (frame, stereo) in actual.chunks_exact(CHANNELS).enumerate() {
        let expected = expected[frame].clamp(-1.0, 1.0);
        for (channel, &sample) in stereo.iter().enumerate() {
            let error = (sample - expected).abs();
            if !sample.is_finite() || error > 0.000_001 {
                return Err(format!(
                    "independent cooked PCM mismatch at frame={frame} channel={channel}: actual={sample} expected={expected}"
                ));
            }
            maximum_absolute_error = maximum_absolute_error.max(error);
            nonzero_samples += usize::from(sample != 0.0);
        }
    }
    if nonzero_samples == 0 {
        return Err("complete cooked callback output was silent".into());
    }
    Ok(json!({
        "basis": "original immutable acquired SoundBank PCM and independent cause/lifecycle/mix arithmetic",
        "source_processed_equality": "requires separate independent source/processed acceptance",
        "sample_encoding": "f32-le",
        "sample_rate": SAMPLE_RATE,
        "channels": CHANNELS,
        "frames": snapshot.output_frames,
        "samples_compared": actual.len(),
        "nonzero_samples": nonzero_samples,
        "maximum_absolute_error": maximum_absolute_error,
        "callback_blake3": digest(actual),
        "pickup_source_frames": pickup.len(),
        "pickup_source_blake3": digest(pickup),
        "travel_source_frames": travel.len(),
        "travel_source_blake3": digest(travel),
        "accepted_pickups": 3,
        "accepted_travels": 4,
        "stale_scope_cancellations": stale_scope_cancellations,
        "scope_applications": scopes.len(),
        "validated_scene_instances": instances.len(),
    }))
}

#[cfg(test)]
#[path = "oracle/tests.rs"]
mod tests;
