use super::*;

#[test]
fn wav_preserves_the_payload_and_decoded_frames() {
    for channels in [1, 2] {
        let wav = wav(1, channels, 48_000, 16, &[0, 8, 0, 16, 0, 24, 0, 32]);
        let cooked = cook_audio(&wav).unwrap();
        let frame_count = 4 / u64::from(channels);
        let mut expected_header = b"NAUD\x01\0\x02\0\x80\xbb\0\0".to_vec();
        expected_header.extend_from_slice(&channels.to_le_bytes());
        expected_header.extend_from_slice(&[0, 0]);
        expected_header.extend_from_slice(&frame_count.to_le_bytes());
        assert_eq!(&cooked[..NAUD_HEADER_BYTES], expected_header);
        assert_eq!(&cooked[NAUD_HEADER_BYTES..], wav);
        assert_eq!(cooked_audio_codec(&cooked).unwrap(), AudioCodec::PcmWav);
        assert_eq!(
            AudioClip::from_cooked(&cooked).unwrap().0,
            AudioClip::from_bytes(&wav).unwrap().0
        );
    }
}

#[test]
fn rejects_invalid_cooked_headers_and_mismatched_payloads() {
    let wav = wav(1, 1, 48_000, 16, &[0, 8]);
    let cooked = cook_audio(&wav).unwrap();
    let header = &cooked[..NAUD_HEADER_BYTES];
    assert_eq!(cooked_audio_codec(header).unwrap(), AudioCodec::PcmWav);
    assert!(cooked_audio_codec(&header[..header.len() - 1]).is_err());
    assert!(AudioClip::from_cooked(header).is_err());
    assert!(AudioClip::from_cooked(&cooked[..cooked.len() - 1]).is_err());

    for (offset, bytes) in [
        (0, b"bad!".as_slice()),
        (4, &[2, 0]),
        (6, &[0, 0]),
        (8, &[0; 4]),
        (12, &[3, 0]),
        (14, &[1, 0]),
        (16, &[0; 8]),
    ] {
        let mut invalid = cooked.clone();
        invalid[offset..offset + bytes.len()].copy_from_slice(bytes);
        assert!(
            cooked_audio_codec(&invalid).is_err(),
            "header offset {offset}"
        );
        assert!(
            AudioClip::from_cooked(&invalid).is_err(),
            "header offset {offset}"
        );
    }
    for (offset, value) in [(6, 1), (8, 0x81), (12, 2), (16, 2)] {
        let mut mismatched = cooked.clone();
        mismatched[offset] = value;
        assert!(
            cooked_audio_codec(&mismatched).is_ok(),
            "header offset {offset}"
        );
        assert!(matches!(
            AudioClip::from_cooked(&mismatched),
            Err(AudioClipError::InvalidCooked(_))
        ));
    }
}

#[test]
fn rejects_empty_nonfinite_and_partial_wav_samples() {
    assert!(matches!(
        cook_audio(&wav(1, 1, 48_000, 16, &[])),
        Err(AudioClipError::Empty)
    ));
    assert!(matches!(
        cook_audio(&wav(3, 1, 48_000, 32, &f32::NAN.to_le_bytes())),
        Err(AudioClipError::NonFiniteSamples)
    ));
    assert!(cook_audio(&wav(1, 2, 48_000, 16, &[0; 5])).is_err());
    assert!(cook_audio(&wav(1, 3, 48_000, 16, &[0; 6])).is_err());
    assert!(cook_audio(&wav(1, 1, 0, 16, &[0; 2])).is_err());
    assert!(cook_audio(&wav(7, 1, 48_000, 8, &[0; 2])).is_err());
}

#[test]
fn vorbis_round_trips_and_rejects_missing_or_corrupt_pages() {
    // FFmpeg/libvorbis encoded a 160 ms, 8 kHz mono sine with several audio pages.
    let ogg: Vec<_> = include_str!("../../tests/fixtures/tone.ogg.hex")
        .split_whitespace()
        .flat_map(|line| {
            (0..line.len())
                .step_by(2)
                .map(move |i| u8::from_str_radix(&line[i..i + 2], 16).unwrap())
        })
        .collect();
    let cooked = cook_audio(&ogg).unwrap();
    assert_eq!(cooked_audio_codec(&cooked).unwrap(), AudioCodec::OggVorbis);
    assert_eq!(&cooked[8..14], b"\x40\x1f\0\0\x01\0");
    assert_eq!(&cooked[NAUD_HEADER_BYTES..], ogg);
    assert_eq!(
        AudioClip::from_cooked(&cooked).unwrap().0,
        AudioClip::from_bytes(&ogg).unwrap().0
    );

    let last_page = ogg.windows(4).rposition(|bytes| bytes == b"OggS").unwrap();
    assert!(cook_audio(&ogg[..last_page]).is_err());
    assert!(AudioClip::from_cooked(&cooked[..cooked.len() - 1]).is_err());
    let mut corrupt = ogg;
    corrupt[last_page - 1] ^= 1;
    assert!(cook_audio(&corrupt).is_err());
}

fn wav(format: u16, channels: u16, rate: u32, bits: u16, samples: &[u8]) -> Vec<u8> {
    let len = samples.len() as u32;
    let padding = len % 2;
    let block_align = channels * (bits / 8);
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&(36 + len + padding).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0");
    wav.extend_from_slice(&format.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&len.to_le_bytes());
    wav.extend_from_slice(samples);
    if padding != 0 {
        wav.push(0);
    }
    wav
}
