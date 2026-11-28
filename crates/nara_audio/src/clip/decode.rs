use std::io::Cursor;

use kira::{Frame, sound::static_sound::StaticSoundData};
use symphonia::{
    core::{
        checksum::Crc32,
        codecs::audio::well_known::{
            CODEC_ID_PCM_F32LE, CODEC_ID_PCM_F64LE, CODEC_ID_PCM_S16LE, CODEC_ID_PCM_S24LE,
            CODEC_ID_PCM_S32LE, CODEC_ID_PCM_U8, CODEC_ID_VORBIS,
        },
        formats::TrackType,
        io::{MediaSourceStream, Monitor},
    },
    default::{get_codecs, get_probe},
};

use super::{AudioClip, AudioClipError, AudioCodec, AudioMetadata};

pub(super) fn source(bytes: &[u8]) -> Result<(AudioClip, AudioMetadata), AudioClipError> {
    let wav_bytes = if bytes.starts_with(b"OggS") {
        validate_ogg_pages(bytes)?;
        None
    } else if bytes.starts_with(b"RIFF") {
        Some(wav_data_bytes(bytes)?)
    } else {
        return Err(decode_error("expected Ogg Vorbis or PCM WAV"));
    };

    let stream = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut format = get_probe()
        .probe(
            &Default::default(),
            stream,
            Default::default(),
            Default::default(),
        )
        .map_err(symphonia_error)?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| decode_error("missing audio track"))?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|params| params.audio())
        .ok_or_else(|| decode_error("missing audio codec parameters"))?;
    let sample_rate = params
        .sample_rate
        .filter(|rate| *rate != 0)
        .ok_or_else(|| decode_error("invalid sample rate"))?;
    let channels = match params.channels.as_ref().map(|channels| channels.count()) {
        Some(1) => 1_u16,
        Some(2) => 2,
        _ => return Err(decode_error("expected mono or stereo audio")),
    };

    let (codec, expected_frames) = if let Some(wav_bytes) = wav_bytes {
        let sample_bytes = match params.codec {
            CODEC_ID_PCM_U8 => 1,
            CODEC_ID_PCM_S16LE => 2,
            CODEC_ID_PCM_S24LE => 3,
            CODEC_ID_PCM_S32LE | CODEC_ID_PCM_F32LE => 4,
            CODEC_ID_PCM_F64LE => 8,
            _ => return Err(decode_error("expected uncompressed PCM in WAV")),
        };
        let frame_bytes = sample_bytes * u64::from(channels);
        if !wav_bytes.is_multiple_of(frame_bytes) {
            return Err(decode_error("WAV data ends in a partial frame"));
        }
        (AudioCodec::PcmWav, Some(wav_bytes / frame_bytes))
    } else if params.codec == CODEC_ID_VORBIS {
        (AudioCodec::OggVorbis, None)
    } else {
        return Err(decode_error("expected Vorbis in Ogg"));
    };

    let mut decoder = get_codecs()
        .make_audio_decoder(params, &Default::default())
        .map_err(symphonia_error)?;
    let mut samples = Vec::<f32>::new();
    let mut frames = Vec::new();
    while let Some(packet) = format.next_packet().map_err(symphonia_error)? {
        if packet.track_id != track_id {
            return Err(decode_error("unexpected audio track"));
        }
        let audio = decoder.decode(&packet).map_err(symphonia_error)?;
        if audio.spec().rate() != sample_rate || audio.num_planes() != usize::from(channels) {
            return Err(decode_error("audio format changed during decoding"));
        }
        audio.copy_to_vec_interleaved(&mut samples);
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioClipError::NonFiniteSamples);
        }
        frames.try_reserve(samples.len() / usize::from(channels))?;
        if channels == 1 {
            frames.extend(samples.iter().copied().map(Frame::from_mono));
        } else {
            frames.extend(
                samples
                    .chunks_exact(2)
                    .map(|pair| Frame::new(pair[0], pair[1])),
            );
        }
    }
    if frames.is_empty() {
        return Err(AudioClipError::Empty);
    }
    let frame_count = frames.len() as u64;
    if expected_frames.is_some_and(|expected| expected != frame_count) {
        return Err(decode_error(
            "WAV frame count does not match its data length",
        ));
    }

    let clip = AudioClip(StaticSoundData {
        sample_rate,
        frames: frames.into(),
        settings: Default::default(),
        slice: None,
    });
    Ok((
        clip,
        AudioMetadata {
            codec,
            sample_rate,
            channels,
            frame_count,
        },
    ))
}

fn wav_data_bytes(bytes: &[u8]) -> Result<u64, AudioClipError> {
    let mut riff = bytes
        .strip_prefix(b"RIFF")
        .and_then(<[u8]>::split_first_chunk::<4>)
        .filter(|(len, data)| u32::from_le_bytes(**len) as usize == data.len())
        .and_then(|(_, data)| data.strip_prefix(b"WAVE"))
        .ok_or_else(|| decode_error("invalid or truncated RIFF/WAVE header"))?;
    let mut data_bytes = None;
    while !riff.is_empty() {
        let (tag, chunk) = riff
            .split_first_chunk::<4>()
            .ok_or_else(|| decode_error("truncated WAV chunk tag"))?;
        let (len, chunk) = chunk
            .split_first_chunk::<4>()
            .ok_or_else(|| decode_error("truncated WAV chunk length"))?;
        let len = u32::from_le_bytes(*len);
        let (_, rest) = chunk
            .split_at_checked(len as usize)
            .ok_or_else(|| decode_error("truncated WAV chunk"))?;
        if tag == b"data" && data_bytes.replace(u64::from(len)).is_some() {
            return Err(decode_error("multiple WAV data chunks"));
        }
        // RIFF aligns the next chunk to an even byte offset.
        riff = if len % 2 != 0 && !rest.is_empty() {
            &rest[1..]
        } else {
            rest
        };
    }
    data_bytes.ok_or_else(|| decode_error("missing WAV data chunk"))
}

fn validate_ogg_pages(mut bytes: &[u8]) -> Result<(), AudioClipError> {
    let serial = bytes.get(14..18);
    let mut sequence = 0_u32;
    let mut continued = false;
    while !bytes.is_empty() {
        let (header, rest) = bytes
            .split_first_chunk::<27>()
            .ok_or_else(|| decode_error("truncated Ogg page header"))?;
        let flags = header[5];
        // A static clip contains one complete logical stream. The demuxer can otherwise
        // recover by dropping corrupt pages, hiding missing samples from the cooker.
        if &header[..5] != b"OggS\0"
            || flags & !0x07 != 0
            || (flags & 0x01 != 0) != continued
            || (flags & 0x02 != 0) != (sequence == 0)
            || Some(&header[14..18]) != serial
            || header[18..22] != sequence.to_le_bytes()
        {
            return Err(decode_error("invalid or missing Ogg page"));
        }
        let (segments, body) = rest
            .split_at_checked(usize::from(header[26]))
            .ok_or_else(|| decode_error("truncated Ogg segment table"))?;
        let body_len = segments.iter().map(|&size| usize::from(size)).sum();
        let (body, rest) = body
            .split_at_checked(body_len)
            .ok_or_else(|| decode_error("truncated Ogg page body"))?;
        let mut checksum = Crc32::new(0);
        checksum.process_buf_bytes(&header[..22]);
        checksum.process_buf_bytes(&[0; 4]);
        checksum.process_buf_bytes(&header[26..]);
        checksum.process_buf_bytes(segments);
        checksum.process_buf_bytes(body);
        if header[22..26] != checksum.crc().to_le_bytes() {
            return Err(decode_error("Ogg page checksum mismatch"));
        }
        if let Some(&last) = segments.last() {
            continued = last == 255;
        }
        if flags & 0x04 != 0 {
            return if rest.is_empty() && !continued {
                Ok(())
            } else {
                Err(decode_error("invalid Ogg end-of-stream page"))
            };
        }
        sequence = sequence.wrapping_add(1);
        bytes = rest;
    }
    Err(decode_error("missing Ogg end-of-stream page"))
}

fn decode_error(message: &str) -> AudioClipError {
    AudioClipError::Decode(message.to_owned())
}

fn symphonia_error(error: symphonia::core::errors::Error) -> AudioClipError {
    AudioClipError::Decode(error.to_string())
}
