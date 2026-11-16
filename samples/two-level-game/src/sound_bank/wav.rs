//! Complete RIFF preflight before allocating any decoded clip storage.
//!
//! Hound receives borrowed format/data chunks after structural admission. Unknown chunks and
//! their word padding are validated independently, including every chunk after the audio data.

use std::{
    io::{Cursor, Read},
    sync::Arc,
};

use super::{MAX_CLIP_FRAMES, MAX_WAV_BYTES, Rejection, SAMPLE_RATE};

pub(super) fn preflight(bytes: &[u8]) -> Result<&[u8], Rejection> {
    if bytes.len() > MAX_WAV_BYTES {
        return Err(Rejection::WavBounds);
    }
    let header = bytes.get(..12).ok_or(Rejection::WavShape)?;
    if &header[..4] != b"RIFF"
        || &header[8..12] != b"WAVE"
        || u64::from(u32_at(header, 4)) + 8 != bytes.len() as u64
    {
        return Err(Rejection::WavShape);
    }
    let mut offset = 12_usize;
    let mut format = None;
    let mut pcm = None;
    while offset < bytes.len() {
        let header = bytes.get(offset..offset + 8).ok_or(Rejection::WavShape)?;
        let length = u32_at(header, 4) as usize;
        let start = offset + 8;
        let end = start.checked_add(length).ok_or(Rejection::WavShape)?;
        let payload = bytes.get(start..end).ok_or(Rejection::WavShape)?;
        offset = end.checked_add(length & 1).ok_or(Rejection::WavShape)?;
        if offset > bytes.len() {
            return Err(Rejection::WavShape);
        }
        match &header[..4] {
            b"fmt " => {
                if format.is_some() || pcm.is_some() {
                    return Err(Rejection::WavShape);
                }
                if !matches!(payload.len(), 16 | 18)
                    || (payload.len() == 18 && payload[16..] != [0, 0])
                    || u16_at(payload, 0) != 1
                    || u16_at(payload, 12) != 2
                {
                    return Err(Rejection::WavFormat);
                }
                format = Some(&bytes[start - 8..end]);
            }
            b"data" => {
                if format.is_none() || pcm.is_some() || !length.is_multiple_of(2) {
                    return Err(Rejection::WavShape);
                }
                if length == 0 {
                    return Err(Rejection::WavEmpty);
                }
                if length / 2 > MAX_CLIP_FRAMES {
                    return Err(Rejection::WavFrames);
                }
                pcm = Some((&bytes[start - 8..end], payload));
            }
            _ => {}
        }
    }
    let format = format.ok_or(Rejection::WavShape)?;
    let (data, pcm) = pcm.ok_or(Rejection::WavShape)?;
    let input = Cursor::new(&bytes[..12])
        .chain(Cursor::new(format))
        .chain(Cursor::new(data));
    let reader = hound::WavReader::new(input).map_err(|_| Rejection::WavFormat)?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != SAMPLE_RATE
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() as usize != pcm.len() / 2
    {
        return Err(Rejection::WavFormat);
    }
    Ok(pcm)
}

pub(super) fn samples(pcm: &[u8]) -> Arc<[f32]> {
    pcm.chunks_exact(2)
        .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0)
        .collect::<Vec<_>>()
        .into()
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
