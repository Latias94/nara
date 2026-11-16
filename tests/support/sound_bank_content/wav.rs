use std::ops::Range;

pub fn pcm16_data(bytes: &[u8]) -> Range<usize> {
    assert!(bytes.len() >= 12 && bytes.len() <= 1024 * 1024);
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(
        u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
        bytes.len()
    );
    let mut offset = 12;
    let mut format = None;
    let mut data = None;
    while offset < bytes.len() {
        let header = bytes
            .get(offset..offset + 8)
            .expect("complete WAV chunk header");
        let length = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .unwrap();
        match &header[..4] {
            b"fmt " => {
                assert!(format.is_none() && length >= 16);
                let spec = &bytes[start..end];
                assert_eq!(u16::from_le_bytes(spec[..2].try_into().unwrap()), 1);
                assert_eq!(u16::from_le_bytes(spec[2..4].try_into().unwrap()), 1);
                assert_eq!(u32::from_le_bytes(spec[4..8].try_into().unwrap()), 48_000);
                assert_eq!(u32::from_le_bytes(spec[8..12].try_into().unwrap()), 96_000);
                assert_eq!(u16::from_le_bytes(spec[12..14].try_into().unwrap()), 2);
                assert_eq!(u16::from_le_bytes(spec[14..16].try_into().unwrap()), 16);
                format = Some(());
            }
            b"data" => {
                assert!(data.is_none() && length > 0 && length % 2 == 0);
                data = Some(start..end);
            }
            _ => {}
        }
        offset = end.checked_add(length % 2).unwrap();
    }
    assert_eq!(offset, bytes.len());
    assert!(format.is_some());
    data.expect("one PCM16 data chunk")
}

pub fn decode(bytes: &[u8]) -> Vec<f32> {
    let samples = bytes[pcm16_data(bytes)]
        .chunks_exact(2)
        .map(|sample| f32::from(i16::from_le_bytes(sample.try_into().unwrap())) / 32768.0)
        .collect::<Vec<_>>();
    assert!(samples.iter().any(|sample| *sample != 0.0));
    samples
}
