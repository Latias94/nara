use std::{collections::BTreeMap, io::Read, ops::Range};

use serde::Serialize;
use two_level_game::{
    audio::{AudioCause, AudioClip, AudioObservation, AudioPhase, AudioRecord},
    levels::LevelKey,
};

pub(super) const SAMPLE_RATE: u64 = 48_000;
pub(super) const CHANNELS: usize = 2;
pub(super) const MAX_OUTPUT_FRAMES: u64 = SAMPLE_RATE * 60;

#[derive(Clone, Copy)]
pub(super) struct SourceWavs<'a> {
    pub pickup: &'a [u8],
    pub travel: &'a [u8],
}

#[derive(Debug, Serialize)]
pub(super) struct PcmEvidence {
    pub frames: u64,
    pub samples_compared: usize,
    pub nonzero_samples: usize,
    pub maximum_absolute_error: f32,
    pub accepted_pickups: usize,
    pub accepted_travels: usize,
    pub stale_scope_cancellations: usize,
    pub scope_applications: usize,
    pub validated_scene_instances: usize,
    pub pickup_source_frames: usize,
    pub travel_source_frames: usize,
}

struct Cue {
    clip: AudioClip,
    cause: AudioCause,
    accepted: u64,
    start: Option<u64>,
    end: Option<(u64, bool)>,
}

/// Isolates the PCM chunks. Hound 3.5.1 does not skip RIFF padding on odd ancillary chunks.
pub(super) fn wav_chunks(bytes: &[u8]) -> Result<(&[u8], Range<usize>), String> {
    let mut offset = 12;
    let mut format = None;
    while let Some(header) = bytes.get(offset..offset + 8) {
        let length = u32::from_le_bytes(header[4..].try_into().expect("chunk size field"));
        let end = (offset + 8)
            .checked_add(length as usize)
            .ok_or("independent source WAV chunk length overflowed")?;
        let chunk = bytes
            .get(offset..end)
            .ok_or("independent source WAV has a truncated chunk")?;
        match &header[..4] {
            b"fmt " => format = Some(chunk),
            b"data" => {
                return Ok((
                    format.ok_or("independent source WAV has no format before its data")?,
                    offset..end,
                ));
            }
            _ => {}
        }
        offset = end + (length as usize % 2);
    }
    Err("independent source WAV has no data chunk".into())
}

/// Decodes the captured PCM16 files independently of the game's prepared clips and mixer.
fn decode(bytes: &[u8]) -> Result<Vec<f32>, String> {
    let (format, data) = wav_chunks(bytes)?;
    let input = (&bytes[..12]).chain(format).chain(&bytes[data]);
    let mut wav = hound::WavReader::new(input).map_err(|error| error.to_string())?;
    let spec = wav.spec();
    if spec.sample_rate != SAMPLE_RATE as u32
        || spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(format!(
            "independent source WAV has an unsupported format: {spec:?}"
        ));
    }
    let samples = wav
        .samples::<i16>()
        .map(|sample| sample.map(|value| f32::from(value) / 32768.0))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if samples.is_empty() {
        return Err("independent source WAV is empty".into());
    }
    Ok(samples)
}

fn source_clip(cause: &AudioCause) -> AudioClip {
    match cause {
        AudioCause::Pickup { .. } => AudioClip::Pickup,
        AudioCause::Travel { .. } => AudioClip::Travel,
    }
}

pub(super) fn verify(
    sources: SourceWavs<'_>,
    observation: &AudioObservation,
    actual: &[f32],
    expected_causes: &[AudioCause],
    instances: &BTreeMap<u64, LevelKey>,
) -> Result<PcmEvidence, String> {
    let snapshot = &observation.snapshot;
    if snapshot.error.is_some() || snapshot.faults != 0 {
        return Err(format!(
            "audio observation failed: {:?}; faults={:#x}",
            snapshot.error, snapshot.faults
        ));
    }
    if !snapshot.play_admitted
        || !snapshot.physical_complete
        || !snapshot.stream_drop_returned
        || !snapshot.data_callback_released
        || !snapshot.error_callback_released
        || snapshot.admission_open
    {
        return Err(
            "PCM acceptance requires the original audio owner to be physically retired".into(),
        );
    }
    if snapshot.callbacks == 0
        || snapshot.output_frames == 0
        || snapshot.output_frames > MAX_OUTPUT_FRAMES
        || actual.len() as u64 != snapshot.output_frames * CHANNELS as u64
    {
        return Err("callback capture does not cover the complete bounded output interval".into());
    }
    if observation.evicted_records != 0 {
        return Err(format!(
            "audio diagnostic history is incomplete (evicted records = {})",
            observation.evicted_records
        ));
    }

    let pickup = decode(sources.pickup).map_err(|error| format!("Pickup: {error}"))?;
    let travel = decode(sources.travel).map_err(|error| format!("Travel: {error}"))?;
    let source_samples = |clip| -> &[f32] {
        match clip {
            AudioClip::Pickup => &pickup,
            AudioClip::Travel => &travel,
        }
    };
    let mut cues: BTreeMap<(u64, u64, u64), Cue> = BTreeMap::new();
    let mut scopes = Vec::new();
    let mut accepted_causes = Vec::new();
    let mut starts = 0;
    let mut completions = 0;
    let mut cancellations = 0;
    let mut last_sequence = 0;
    for record in &snapshot.records {
        match record {
            AudioRecord::ScopeApplied {
                runtime,
                instance,
                frame,
            } => {
                if *runtime != snapshot.runtime
                    || *instance == 0
                    || *frame > snapshot.output_frames
                    || !instances.contains_key(instance)
                {
                    return Err(
                        "scope application has a foreign identity or output boundary".into(),
                    );
                }
                if scopes
                    .last()
                    .is_some_and(|&(prior, at)| prior >= *instance || at > *frame)
                {
                    return Err("callback scope identity or output position regressed".into());
                }
                scopes.push((*instance, *frame));
            }
            AudioRecord::Cue {
                runtime,
                instance,
                sequence,
                clip,
                cause,
                frame,
                phase,
            } => {
                if *runtime != snapshot.runtime
                    || *instance == 0
                    || *sequence == 0
                    || *frame > snapshot.output_frames
                    || *clip != source_clip(cause)
                {
                    return Err(format!(
                        "cue disagrees with its original gameplay cause: {record:?}"
                    ));
                }
                let key = (*runtime, *instance, *sequence);
                match phase {
                    AudioPhase::Accepted => {
                        let level = match cause {
                            AudioCause::Pickup { level, .. } | AudioCause::Travel { level } => {
                                level
                            }
                        };
                        if instances.get(instance) != Some(level)
                            || scopes.last().map(|&(applied, _)| applied) != Some(*instance)
                            || *sequence <= last_sequence
                        {
                            return Err("accepted cue does not belong to its actual game Scene instance and event order".into());
                        }
                        last_sequence = *sequence;
                        if cues
                            .insert(
                                key,
                                Cue {
                                    clip: *clip,
                                    cause: cause.clone(),
                                    accepted: *frame,
                                    start: None,
                                    end: None,
                                },
                            )
                            .is_some()
                        {
                            return Err("one game effect was accepted more than once".into());
                        }
                        accepted_causes.push(cause.clone());
                    }
                    AudioPhase::Started | AudioPhase::Completed | AudioPhase::Cancelled { .. } => {
                        let cue = cues
                            .get_mut(&key)
                            .ok_or("callback reported a cue before acceptance")?;
                        if cue.clip != *clip || cue.cause != *cause || *frame < cue.accepted {
                            return Err("cue identity changed between callback facts".into());
                        }
                        match phase {
                            AudioPhase::Started => {
                                if scopes.last().map(|&(applied, _)| applied) != Some(*instance)
                                    || cue.start.replace(*frame).is_some()
                                    || cue.end.is_some()
                                {
                                    return Err(
                                        "one game effect started twice or after retirement".into(),
                                    );
                                }
                                starts += 1;
                            }
                            AudioPhase::Completed => {
                                let start = cue.start.ok_or("unstarted cue reported completion")?;
                                let expected_end = start + source_samples(*clip).len() as u64;
                                if *frame != expected_end {
                                    return Err(format!(
                                        "{clip:?} cue completion at frame={frame} differs from its source WAV boundary at frame={expected_end}"
                                    ));
                                }
                                if cue.end.replace((*frame, false)).is_some() {
                                    return Err("cue completed after a terminal fact".into());
                                }
                                completions += 1;
                            }
                            AudioPhase::Cancelled { started } => {
                                if *started != cue.start.is_some()
                                    || cue.start.is_some_and(|start| *frame < start)
                                    || cue.end.replace((*frame, true)).is_some()
                                {
                                    return Err("cue cancellation conflicts with its start or terminal fact".into());
                                }
                                cancellations += 1;
                            }
                            _ => unreachable!(),
                        }
                    }
                    AudioPhase::Duplicate | AudioPhase::Refused { .. } => {
                        return Err(format!(
                            "ordinary gameplay produced a duplicate or refused cue: {record:?}"
                        ));
                    }
                }
            }
        }
    }

    if accepted_causes.as_slice() != expected_causes {
        return Err(format!(
            "committed gameplay causes differ from the commanded trajectory: {accepted_causes:?}"
        ));
    }
    if snapshot.accepted != cues.len() as u64
        || snapshot.started != starts
        || snapshot.completed != completions
        || snapshot.cancelled != cancellations
        || completions + cancellations != snapshot.accepted
    {
        return Err("native counters disagree with the complete ordered cue lifecycle".into());
    }
    if scopes.last().map(|&(instance, _)| instance) != Some(snapshot.applied_instance)
        || snapshot.desired_instance != snapshot.applied_instance
    {
        return Err("final native scope differs from its applied callback history".into());
    }

    let mut expected = vec![0.0_f32; snapshot.output_frames as usize];
    let mut stale_scope_cancellations = 0;
    for ((_, instance, _), cue) in cues {
        let (end, cancelled) = cue
            .end
            .ok_or("accepted cue has no callback terminal fact")?;
        let first_replacement = scopes
            .iter()
            .find(|&&(next, _)| next > instance)
            .map(|&(_, frame)| frame);
        if first_replacement.is_some_and(|cut| end > cut) {
            return Err("voice survived its first original Scene scope replacement".into());
        }
        if cancelled {
            if first_replacement != Some(end) {
                return Err(
                    "cancellation has no matching replacement scope at its output boundary".into(),
                );
            }
            stale_scope_cancellations += 1;
        }
        if let Some(start) = cue.start {
            let source = source_samples(cue.clip);
            if end < start
                || end - start > source.len() as u64
                || (cancelled && end - start == source.len() as u64)
            {
                return Err(
                    "voice extends beyond its original WAV or cancellation boundary".into(),
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
            if !sample.is_finite() {
                return Err(format!(
                    "nonfinite callback sample at frame={frame} channel={channel}"
                ));
            }
            let error = (sample - expected).abs();
            maximum_absolute_error = maximum_absolute_error.max(error);
            nonzero_samples += usize::from(sample != 0.0);
            if error > 0.000_001 {
                return Err(format!(
                    "independent PCM mismatch at frame={frame} channel={channel}: actual={sample} expected={expected}"
                ));
            }
        }
    }
    Ok(PcmEvidence {
        frames: snapshot.output_frames,
        samples_compared: actual.len(),
        nonzero_samples,
        maximum_absolute_error,
        accepted_pickups: accepted_causes
            .iter()
            .filter(|cause| matches!(cause, AudioCause::Pickup { .. }))
            .count(),
        accepted_travels: accepted_causes
            .iter()
            .filter(|cause| matches!(cause, AudioCause::Travel { .. }))
            .count(),
        stale_scope_cancellations,
        scope_applications: scopes.len(),
        validated_scene_instances: instances.len(),
        pickup_source_frames: pickup.len(),
        travel_source_frames: travel.len(),
    })
}
