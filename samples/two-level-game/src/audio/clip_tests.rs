use super::*;

/// Independently write the small canonical WAV header; do not use the decoder's writer as oracle.
fn wav_bytes(format: u16, channels: u16, rate: u32, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let align = channels * (bits / 8);
    let padding = pcm.len() % 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + pcm.len() as u32 + padding as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&format.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    bytes.extend_from_slice(&align.to_le_bytes());
    bytes.extend_from_slice(&bits.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    bytes.extend_from_slice(pcm);
    if padding != 0 {
        bytes.push(0);
    }
    bytes
}

#[test]
fn pcm16_extremes_match_independent_bytes_exactly() {
    let pcm = [
        0x00, 0x80, 0x00, 0xc0, 0xff, 0xff, 0, 0, 1, 0, 0, 0x40, 0xff, 0x7f,
    ];
    let bytes = wav_bytes(1, 1, 48_000, 16, &pcm);
    let clips = PreparedClips::decode(&bytes, &bytes).unwrap();
    let expected = [
        -1.0,
        -0.5,
        -1.0 / 32_768.0,
        0.0,
        1.0 / 32_768.0,
        0.5,
        32_767.0 / 32_768.0,
    ];
    assert_eq!(clips.samples(ClipKind::Pickup), expected);
    assert_eq!(clips.samples(ClipKind::Travel), expected);
}

#[test]
fn every_embedded_sample_matches_the_original_pcm_bytes() {
    let clips = PreparedClips::embedded().unwrap();
    for (clip, wav, frames) in [
        (ClipKind::Pickup, PICKUP_WAV, 5_760),
        (ClipKind::Travel, TRAVEL_WAV, 11_520),
    ] {
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(&wav[16..24], &[16, 0, 0, 0, 1, 0, 1, 0]);
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 48_000);
        assert_eq!(u32::from_le_bytes(wav[28..32].try_into().unwrap()), 96_000);
        assert_eq!(&wav[32..40], &[2, 0, 16, 0, b'd', b'a', b't', b'a']);
        assert_eq!(
            u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize,
            frames * 2
        );
        assert_eq!(
            u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize,
            wav.len() - 8
        );
        assert_eq!(wav.len(), 44 + frames * 2);
        let expected: Vec<f32> = wav[44..]
            .chunks_exact(2)
            .map(|sample| f32::from(i16::from_le_bytes([sample[0], sample[1]])) / 32_768.0)
            .collect();
        assert_eq!(clips.samples(clip), expected);
        assert_eq!(expected.first(), Some(&0.0));
        assert_eq!(expected.last(), Some(&0.0));
        assert!(expected.iter().any(|sample| sample.abs() > 0.1));
    }
}

#[test]
fn unsupported_rates_channels_and_sample_formats_refuse_preparation() {
    for (format, channels, rate, bits, pcm) in [
        (1, 1, 44_100, 16, vec![0, 0]),
        (1, 1, 0, 16, vec![0, 0]),
        (1, 2, 48_000, 16, vec![0, 0, 0, 0]),
        (1, 1, 48_000, 8, vec![128]),
        (3, 1, 48_000, 32, 0.5_f32.to_le_bytes().to_vec()),
    ] {
        let bytes = wav_bytes(format, channels, rate, bits, &pcm);
        assert!(matches!(
            decode_wav(ClipKind::Travel, &bytes),
            Err(ClipError::UnsupportedFormat {
                clip: ClipKind::Travel,
                ..
            })
        ));
    }
}

#[test]
fn empty_truncated_and_malformed_wavs_are_not_silently_accepted() {
    let empty = wav_bytes(1, 1, 48_000, 16, &[]);
    assert!(matches!(
        decode_wav(ClipKind::Pickup, &empty),
        Err(ClipError::Empty { .. })
    ));
    let mut truncated = wav_bytes(1, 1, 48_000, 16, &[0, 0, 1, 0]);
    truncated.pop();
    assert!(matches!(
        decode_wav(ClipKind::Pickup, &truncated),
        Err(ClipError::Decode { .. })
    ));
    assert!(matches!(
        decode_wav(ClipKind::Travel, b"not a WAV"),
        Err(ClipError::Decode { .. })
    ));
}

#[test]
fn oversized_wav_is_refused_before_allocating_or_reading_its_claimed_samples() {
    let mut header = wav_bytes(1, 1, 48_000, 16, &[]);
    let data_bytes = (MAX_CLIP_FRAMES as u32 + 1) * 2;
    header[4..8].copy_from_slice(&(36 + data_bytes).to_le_bytes());
    header[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    assert!(matches!(
        decode_wav(ClipKind::Pickup, &header),
        Err(ClipError::TooLong { frames, .. }) if frames == MAX_CLIP_FRAMES + 1
    ));
}

#[test]
fn direct_samples_are_bounded_nonempty_and_finite_for_both_clips() {
    for kind in [ClipKind::Pickup, ClipKind::Travel] {
        for samples in [
            vec![],
            vec![0.0; MAX_CLIP_FRAMES + 1],
            vec![0.0, f32::NAN],
            vec![0.0, f32::INFINITY],
            vec![0.0, f32::NEG_INFINITY],
            vec![0.0, 1.01],
            vec![0.0, -1.01],
        ] {
            let valid: Arc<[f32]> = Arc::from([0.0]);
            let supplied: Arc<[f32]> = samples.into();
            let result = match kind {
                ClipKind::Pickup => PreparedClips::new(supplied, valid),
                ClipKind::Travel => PreparedClips::new(valid, supplied),
            };
            match result.unwrap_err() {
                ClipError::Empty { clip }
                | ClipError::TooLong { clip, .. }
                | ClipError::InvalidSample { clip, .. } => assert_eq!(clip, kind),
                other => panic!("unexpected preparation error: {other}"),
            }
        }
    }
}

#[test]
fn prepared_clips_keep_the_original_immutable_arc_allocations() {
    let pickup: Arc<[f32]> = Arc::from([-1.0, 0.0, 1.0]);
    let travel: Arc<[f32]> = Arc::from([0.25, -0.25]);
    let clips = PreparedClips::new(pickup.clone(), travel.clone()).unwrap();
    assert!(Arc::ptr_eq(&clips.pickup, &pickup));
    assert!(Arc::ptr_eq(&clips.travel, &travel));
}
