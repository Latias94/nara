use std::{alloc::Layout, sync::atomic::AtomicUsize};

use nara::asset::{
    AssetBuildSource, AssetDomain, AssetImportError, AssetImportSource, AssetMeta,
    AssetMetaIdentity, AssetMetaIdentityCandidate, AssetPath, AssetSourceKind, RuntimeAssetCodec,
    SourceAssetBuild, SourceAssetImporter,
};
use serde::Serialize;

use super::*;

const SOURCE: &[u8] = include_bytes!("../../assets/audio/game.soundbank.json");
const META: &[u8] = include_bytes!("../../assets/audio/game.soundbank.json.meta");
const PICKUP: &[u8] = include_bytes!("../../assets/audio/pickup.wav");
const TRAVEL: &[u8] = include_bytes!("../../assets/audio/travel.wav");
const FOREIGN_SOURCE_ID: &str = "c4d051e8-73ee-4b16-9d13-8c5fcb8a6a32";

#[derive(Debug, PartialEq, Eq)]
enum ReadFailure {
    Missing(u32),
    Bound { actual: usize, maximum: usize },
}

fn metadata(settings: impl Serialize) -> AssetMetaIdentityCandidate {
    metadata_with(
        BANK_SOURCE_ID,
        AssetSourceKind::Other(SoundBankDomain::SOURCE_KIND.to_owned()),
        settings,
    )
}

fn metadata_with(
    source: &str,
    kind: AssetSourceKind,
    settings: impl Serialize,
) -> AssetMetaIdentityCandidate {
    let identity = AssetMetaIdentity::new(
        StableAssetId::parse_str(source).unwrap(),
        AssetPath::new(BANK_PATH).unwrap(),
        kind,
    );
    let encoded = AssetMeta::new(identity, settings).to_json_string().unwrap();
    AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap()
}

fn prepare(pickup: &[u8], travel: &[u8]) -> Result<SoundBank, AssetImportError<ReadFailure>> {
    let metadata = metadata(SoundBankSettings {});
    let mut reader = |path: &AssetPath, _: ByteLimit| {
        let bytes = match path.as_str() {
            "audio/pickup.wav" => pickup,
            "audio/travel.wav" => travel,
            _ => return Err(ReadFailure::Missing(73)),
        };
        Ok(Arc::from(bytes))
    };
    let mut source = AssetImportSource::new(&metadata, Arc::from(SOURCE), &mut reader);
    SoundBankDomain::prepare(&mut source)
}

fn pcm_wav(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut writer = hound::WavWriter::new(
            std::io::Cursor::new(&mut bytes),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for &sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }
    bytes
}

fn repair_riff_size(bytes: &mut [u8]) {
    let length = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&length.to_le_bytes());
}

fn assert_import_code(error: AssetImportError<ReadFailure>, code: &str) {
    match error {
        AssetImportError::Domain(diagnostic) => assert_eq!(diagnostic.code().as_str(), code),
        AssetImportError::Source(error) => panic!("unexpected reader failure: {error:?}"),
    }
}

fn assert_runtime_code(bytes: &[u8], code: &str) {
    let error = SoundBankCodec::decode_runtime(bytes).unwrap_err();
    assert_eq!(error.diagnostic().code().as_str(), code);
}

#[test]
fn committed_bank_imports_only_bounded_raw_dependencies_and_exact_samples() {
    let metadata = AssetMetaIdentityCandidate::decode_json_bytes(META).unwrap();
    assert_eq!(metadata.identity().stable_id(), bank_product().source());
    assert_eq!(metadata.identity().path().as_str(), BANK_PATH);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(SOURCE).unwrap(),
        serde_json::json!({"pickup": "audio/pickup.wav", "travel": "audio/travel.wav"}),
    );
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.as_str().to_owned(), limit.get()));
        let bytes = match path.as_str() {
            "audio/pickup.wav" => PICKUP,
            "audio/travel.wav" => TRAVEL,
            _ => return Err(ReadFailure::Missing(1)),
        };
        if bytes.len() > limit.get() {
            return Err(ReadFailure::Bound {
                actual: bytes.len(),
                maximum: limit.get(),
            });
        }
        Ok(Arc::from(bytes))
    };
    let bank = {
        let mut source = AssetImportSource::new(&metadata, Arc::from(SOURCE), &mut reader);
        SoundBankDomain::prepare(&mut source).unwrap()
    };
    assert_eq!(bank.product(), bank_product());
    assert_eq!(
        requests,
        [
            ("audio/pickup.wav".to_owned(), MAX_WAV_BYTES),
            ("audio/travel.wav".to_owned(), MAX_WAV_BYTES),
        ],
    );
    for (cue, bytes, frames) in [
        (SoundCue::Pickup, PICKUP, 5_760),
        (SoundCue::Travel, TRAVEL, 11_520),
    ] {
        let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
        let expected: Vec<_> = reader
            .samples::<i16>()
            .map(|sample| (f32::from(sample.unwrap()) / 32_768.0).to_bits())
            .collect();
        let actual: Vec<_> = bank
            .samples(cue)
            .iter()
            .map(|sample| sample.to_bits())
            .collect();
        assert_eq!(actual.len(), frames);
        assert_eq!(actual, expected);
        assert_eq!(actual.first(), Some(&0));
        assert_eq!(actual.last(), Some(&0));
    }
}

#[test]
fn settings_accept_only_an_empty_object_through_the_public_importer() {
    assert_eq!(
        serde_json::to_string(&SoundBankDomain::normalize_settings(SoundBankSettings {}).unwrap())
            .unwrap(),
        "{}",
    );
    for settings in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!(true),
        serde_json::json!(0),
        serde_json::json!(""),
        serde_json::json!({"resample": false}),
    ] {
        let metadata = metadata(settings);
        let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, ReadFailure> {
            panic!("invalid settings must reject before any auxiliary read")
        };
        let mut source = AssetImportSource::new(&metadata, Arc::from(SOURCE), &mut reader);
        assert_import_code(
            SoundBankDomain::prepare(&mut source).unwrap_err(),
            "game.sound-bank.settings",
        );
    }
}

#[test]
fn source_preflight_rejects_invalid_shape_paths_duplicates_bounds_and_trailing_data() {
    let mut invalid = vec![
        String::new(),
        "[]".to_owned(),
        r#"["audio/pickup.wav","audio/travel.wav"]"#.to_owned(),
        r#"{"pickup":"audio/pickup.wav"}"#.to_owned(),
        r#"{"pickup":"audio/pickup.wav","travel":"audio/travel.wav","extra":0}"#.to_owned(),
        r#"{"pickup":"audio/pickup.wav","pickup":"audio/travel.wav"}"#.to_owned(),
        r#"{"pickup":[],"travel":"audio/travel.wav"}"#.to_owned(),
        r#"{"pickup":{"nested":[[[[]]]]},"travel":"audio/travel.wav"}"#.to_owned(),
        r#"{"pickup":"../pickup.wav","travel":"audio/travel.wav"}"#.to_owned(),
        r#"{"pickup":"/pickup.wav","travel":"audio/travel.wav"}"#.to_owned(),
        format!(
            r#"{{"pickup":"{}","travel":"audio/travel.wav"}}"#,
            "a".repeat(129)
        ),
        format!("{} {{}}", std::str::from_utf8(SOURCE).unwrap()),
    ];
    invalid.push(" ".repeat(MAX_SOURCE_BYTES + 1));
    for bytes in invalid {
        let metadata = metadata(SoundBankSettings {});
        let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, ReadFailure> {
            panic!("invalid source must reject before any auxiliary read")
        };
        let mut source =
            AssetImportSource::new(&metadata, Arc::from(bytes.as_bytes()), &mut reader);
        assert_import_code(
            SoundBankDomain::prepare(&mut source).unwrap_err(),
            "game.sound-bank.source",
        );
    }
}

#[test]
fn valid_foreign_source_identity_and_wrong_kind_reject_before_dependency_reads() {
    for metadata in [
        metadata_with(
            FOREIGN_SOURCE_ID,
            AssetSourceKind::Other(SoundBankDomain::SOURCE_KIND.to_owned()),
            SoundBankSettings {},
        ),
        metadata_with(BANK_SOURCE_ID, AssetSourceKind::Image, SoundBankSettings {}),
    ] {
        let mut reader = |_: &AssetPath, _: ByteLimit| -> Result<Arc<[u8]>, ReadFailure> {
            panic!("foreign metadata must reject before any auxiliary read")
        };
        let mut source = AssetImportSource::new(&metadata, Arc::from(SOURCE), &mut reader);
        assert_import_code(
            SoundBankDomain::prepare(&mut source).unwrap_err(),
            "game.sound-bank.identity",
        );
    }
}

#[test]
fn exact_reader_failure_and_its_requested_bound_survive_import() {
    let metadata = metadata(SoundBankSettings {});
    let mut reader = |_: &AssetPath, limit: ByteLimit| {
        Err::<Arc<[u8]>, _>(ReadFailure::Bound {
            actual: limit.get() + 1,
            maximum: limit.get(),
        })
    };
    let mut source = AssetImportSource::new(&metadata, Arc::from(SOURCE), &mut reader);
    match SoundBankDomain::prepare(&mut source).unwrap_err() {
        AssetImportError::Source(error) => assert_eq!(
            error,
            ReadFailure::Bound {
                actual: MAX_WAV_BYTES + 1,
                maximum: MAX_WAV_BYTES
            },
        ),
        AssetImportError::Domain(error) => panic!("unexpected domain failure: {error:?}"),
    }
}

#[test]
fn wav_preflight_refuses_inconsistent_formats_for_either_cue() {
    let original = pcm_wav(&[-32768, -1, 0, 1, 32767]);
    for (offset, replacement) in [
        (20, 3_u32.to_le_bytes()[..2].to_vec()),
        (22, 2_u32.to_le_bytes()[..2].to_vec()),
        (24, 44_100_u32.to_le_bytes().to_vec()),
        (28, 1_u32.to_le_bytes().to_vec()),
        (32, 4_u32.to_le_bytes()[..2].to_vec()),
        (34, 8_u32.to_le_bytes()[..2].to_vec()),
    ] {
        let mut invalid = original.clone();
        invalid[offset..offset + replacement.len()].copy_from_slice(&replacement);
        for (pickup, travel) in [(&invalid[..], &original[..]), (&original[..], &invalid[..])] {
            assert_import_code(
                prepare(pickup, travel).unwrap_err(),
                "game.sound-bank.wav-format",
            );
        }
    }
}

#[test]
fn wav_preflight_validates_complete_riff_and_chunk_lengths_including_after_data() {
    let valid = pcm_wav(&[1, 2, 3]);
    let mut malformed = vec![
        Vec::new(),
        valid[..11].to_vec(),
        valid[..valid.len() - 1].to_vec(),
    ];
    let mut bad_magic = valid.clone();
    bad_magic[0] = b'X';
    malformed.push(bad_magic);
    let mut wrong_riff_size = valid.clone();
    wrong_riff_size[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    malformed.push(wrong_riff_size);
    let mut extra_byte = valid.clone();
    extra_byte.push(0);
    malformed.push(extra_byte);
    let mut trailing_chunk_header = valid.clone();
    trailing_chunk_header.extend_from_slice(b"JUNK");
    repair_riff_size(&mut trailing_chunk_header);
    malformed.push(trailing_chunk_header);
    let mut oversized_chunk = valid.clone();
    oversized_chunk[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
    malformed.push(oversized_chunk);
    let mut odd_data = valid.clone();
    odd_data[40..44].copy_from_slice(&5_u32.to_le_bytes());
    malformed.push(odd_data);
    let mut duplicate_data = valid.clone();
    duplicate_data.extend_from_slice(&valid[36..]);
    repair_riff_size(&mut duplicate_data);
    malformed.push(duplicate_data);
    let mut duplicate_format = valid.clone();
    duplicate_format.extend_from_slice(&valid[12..36]);
    repair_riff_size(&mut duplicate_format);
    malformed.push(duplicate_format);
    let mut missing_format = valid[..12].to_vec();
    missing_format.extend_from_slice(&valid[36..]);
    repair_riff_size(&mut missing_format);
    malformed.push(missing_format);
    let mut missing_data = valid[..36].to_vec();
    repair_riff_size(&mut missing_data);
    malformed.push(missing_data);
    let mut missing_pad = valid.clone();
    missing_pad.extend_from_slice(b"JUNK\x01\0\0\0x");
    repair_riff_size(&mut missing_pad);
    malformed.push(missing_pad);
    for invalid in malformed {
        assert_import_code(
            prepare(&valid, &invalid).unwrap_err(),
            "game.sound-bank.wav-shape",
        );
    }
}

#[test]
fn bounded_ancillary_chunks_are_validated_and_do_not_change_pcm_meaning() {
    let original = pcm_wav(&[i16::MIN, 0, i16::MAX]);
    let expected = prepare(&original, &original).unwrap();
    for offset in [12, 36, original.len()] {
        let mut bytes = original.clone();
        bytes.splice(offset..offset, *b"JUNK\x01\0\0\0x\0");
        repair_riff_size(&mut bytes);
        assert_eq!(prepare(&bytes, &bytes).unwrap(), expected);
    }
}

#[test]
fn pcm_format_extension_must_be_complete_and_empty() {
    let original = pcm_wav(&[i16::MIN, 0, i16::MAX]);
    let mut extended = original.clone();
    extended.splice(36..36, [0, 0]);
    extended[16..20].copy_from_slice(&18_u32.to_le_bytes());
    repair_riff_size(&mut extended);
    assert_eq!(
        prepare(&extended, &extended).unwrap(),
        prepare(&original, &original).unwrap(),
    );
    extended[36] = 1;
    assert_import_code(
        prepare(&original, &extended).unwrap_err(),
        "game.sound-bank.wav-format",
    );
    for length in [0_u32, 14, 15] {
        let mut short_format = original[..20].to_vec();
        short_format[16..20].copy_from_slice(&length.to_le_bytes());
        short_format.extend_from_slice(&original[20..20 + length as usize]);
        if !length.is_multiple_of(2) {
            short_format.push(0);
        }
        short_format.extend_from_slice(&original[36..]);
        repair_riff_size(&mut short_format);
        assert_import_code(
            prepare(&original, &short_format).unwrap_err(),
            "game.sound-bank.wav-format",
        );
    }
}

#[test]
fn wav_empty_frame_limit_and_encoded_bound_are_independent() {
    let valid = pcm_wav(&[0]);
    let maximum = pcm_wav(&vec![i16::MAX; MAX_CLIP_FRAMES]);
    assert_eq!(
        prepare(&maximum, &maximum)
            .unwrap()
            .samples(SoundCue::Travel)
            .len(),
        48_000
    );
    for (invalid, code) in [
        (pcm_wav(&[]), "game.sound-bank.wav-empty"),
        (
            pcm_wav(&vec![0; MAX_CLIP_FRAMES + 1]),
            "game.sound-bank.wav-frames",
        ),
        (vec![0; MAX_WAV_BYTES + 1], "game.sound-bank.wav-bounds"),
    ] {
        assert_import_code(prepare(&valid, &invalid).unwrap_err(), code);
    }
}

#[test]
fn tracked_source_build_uses_the_same_raw_inventory_and_one_complete_product() {
    let metadata = metadata(SoundBankSettings {});
    let pickup = pcm_wav(&[i16::MIN, 0]);
    let travel = pcm_wav(&[1, i16::MAX]);
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.as_str().to_owned(), limit.get()));
        let bytes = match path.as_str() {
            "audio/pickup.wav" => &pickup,
            "audio/travel.wav" => &travel,
            _ => return Err(ReadFailure::Missing(12)),
        };
        Ok(Some(Arc::from(bytes.as_slice())))
    };
    let bank = {
        let mut source = AssetBuildSource::new(&metadata, Arc::from(SOURCE), &mut reader);
        SoundBankDomain::build(&mut source).unwrap()
    };
    assert_eq!(bank, prepare(&pickup, &travel).unwrap());
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], ("audio/pickup.wav".to_owned(), MAX_WAV_BYTES));
    assert_eq!(requests[1], ("audio/travel.wav".to_owned(), MAX_WAV_BYTES));
    let mut products = Vec::new();
    SoundBankDomain::visit_products(&bank, |product| {
        products.push(product);
        Ok::<_, ReadFailure>(())
    })
    .unwrap();
    assert_eq!(products, [bank_product()]);
    assert_eq!(
        SoundBankDomain::visit_products(&bank, |_| Err(ReadFailure::Missing(23))),
        Err(ReadFailure::Missing(23)),
    );
    SoundBankDomain::visit_source_dependencies(&bank, |_, _, _| -> Result<(), ReadFailure> {
        panic!("raw WAVs do not become separately identified Products")
    })
    .unwrap();
    SoundBankCodec::visit_runtime_dependencies(&bank, |_, _| -> Result<(), ReadFailure> {
        panic!("a processed bank has no outgoing Product dependencies")
    })
    .unwrap();
}

#[test]
fn missing_build_dependency_is_required_and_concrete_failures_remain_typed() {
    let metadata = metadata(SoundBankSettings {});
    let mut missing = |_: &AssetPath, _: ByteLimit| Ok::<Option<Arc<[u8]>>, ReadFailure>(None);
    let mut source = AssetBuildSource::new(&metadata, Arc::from(SOURCE), &mut missing);
    assert_import_code(
        SoundBankDomain::build(&mut source).unwrap_err(),
        "asset.build.required-input-missing",
    );
    let mut failure =
        |_: &AssetPath, _: ByteLimit| Err::<Option<Arc<[u8]>>, _>(ReadFailure::Missing(91));
    let mut source = AssetBuildSource::new(&metadata, Arc::from(SOURCE), &mut failure);
    assert!(matches!(
        SoundBankDomain::build(&mut source),
        Err(AssetImportError::Source(ReadFailure::Missing(91))),
    ));
}

#[test]
fn runtime_round_trip_preserves_every_pcm16_value_bit_exactly() {
    let pickup: Vec<_> = (i16::MIN..=-1).collect();
    let travel: Vec<_> = (0..=i16::MAX).collect();
    let source = prepare(&pcm_wav(&pickup), &pcm_wav(&travel)).unwrap();
    let encoded = SoundBankCodec::encode_runtime(&source).unwrap();
    let decoded = SoundBankCodec::decode_runtime(&encoded).unwrap();
    assert_eq!(decoded, source);
    assert_eq!(SoundBankCodec::encode_runtime(&decoded).unwrap(), encoded);
    for (cue, expected) in [(SoundCue::Pickup, pickup), (SoundCue::Travel, travel)] {
        for (&sample, pcm) in decoded.samples(cue).iter().zip(expected) {
            assert!(sample.is_finite());
            assert_eq!(sample.to_bits(), (f32::from(pcm) / 32_768.0).to_bits());
        }
    }
    assert_eq!(decoded.samples(SoundCue::Travel)[0].to_bits(), 0);
}

#[test]
fn runtime_binary_carries_complete_identity_fixed_descriptors_and_only_canonical_pcm() {
    let bank = prepare(&pcm_wav(&[-32768, -1, 0]), &pcm_wav(&[1, 32767])).unwrap();
    let bytes = SoundBankCodec::encode_runtime(&bank).unwrap();
    assert_eq!(&bytes[..8], b"NARASBNK");
    assert_eq!(&bytes[8..12], &1_u32.to_le_bytes());
    assert_eq!(&bytes[12..28], bank_product().source().as_uuid().as_bytes());
    assert_eq!(&bytes[28..45], &[0; 17]);
    assert_eq!(&bytes[45..49], &48_000_u32.to_le_bytes());
    assert_eq!(&bytes[49..56], &[1, 0, 16, 0, 1, 2, 0]);
    assert_eq!(&bytes[56..60], &3_u32.to_le_bytes());
    assert_eq!(&bytes[60..64], &2_u32.to_le_bytes());
    assert_eq!(&bytes[64..], &[0, 128, 255, 255, 0, 0, 1, 0, 255, 127]);
}

#[test]
fn runtime_rejects_actual_foreign_and_auxiliary_identity_in_otherwise_valid_payloads() {
    let bank = prepare(PICKUP, TRAVEL).unwrap();
    let bytes = SoundBankCodec::encode_runtime(&bank).unwrap();
    assert_eq!(SoundBankCodec::decode_runtime(&bytes).unwrap(), bank);
    let mut foreign = bytes.to_vec();
    foreign[12..28].copy_from_slice(
        StableAssetId::parse_str(FOREIGN_SOURCE_ID)
            .unwrap()
            .as_uuid()
            .as_bytes(),
    );
    assert_runtime_code(&foreign, "game.sound-bank.runtime-identity");
    let mut nil = bytes.to_vec();
    nil[12..28].fill(0);
    assert_runtime_code(&nil, "game.sound-bank.runtime-identity");
    let mut auxiliary = bytes.to_vec();
    auxiliary[28] = 1;
    auxiliary[29..45].copy_from_slice(bank_product().source().as_uuid().as_bytes());
    assert_runtime_code(&auxiliary, "game.sound-bank.runtime-identity");
    auxiliary[28] = 0;
    assert_runtime_code(&auxiliary, "game.sound-bank.runtime-identity");
    auxiliary[29..45].fill(0);
    auxiliary[28] = 2;
    assert_runtime_code(&auxiliary, "game.sound-bank.runtime-identity");
    auxiliary[28] = 0;
    assert_eq!(SoundBankCodec::decode_runtime(&auxiliary).unwrap(), bank);
}

#[test]
fn runtime_rejects_truncation_trailing_data_and_unsupported_versions() {
    let bank = prepare(&pcm_wav(&[1]), &pcm_wav(&[-1])).unwrap();
    let bytes = SoundBankCodec::encode_runtime(&bank).unwrap();
    for length in 0..bytes.len() {
        assert!(
            SoundBankCodec::decode_runtime(&bytes[..length]).is_err(),
            "length {length}"
        );
    }
    let mut bad_magic = bytes.to_vec();
    bad_magic[0] ^= 1;
    assert_runtime_code(&bad_magic, "game.sound-bank.runtime-header");
    for version in [0_u32, 2, u32::MAX] {
        let mut unsupported = bytes.to_vec();
        unsupported[8..12].copy_from_slice(&version.to_le_bytes());
        assert_runtime_code(&unsupported, "game.sound-bank.runtime-version");
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert_runtime_code(&trailing, "game.sound-bank.runtime-length");
}

#[test]
fn runtime_rejects_each_foreign_format_descriptor_before_sample_allocation() {
    let bank = prepare(PICKUP, TRAVEL).unwrap();
    let bytes = SoundBankCodec::encode_runtime(&bank).unwrap();
    for offset in [45, 49, 51, 53, 54] {
        let mut changed = bytes.to_vec();
        changed[offset] ^= 1;
        assert_runtime_code(&changed, "game.sound-bank.runtime-format");
    }
}

#[test]
fn runtime_frame_counts_and_encoded_input_are_bounded_independently() {
    let bank = prepare(&pcm_wav(&[1]), &pcm_wav(&[2])).unwrap();
    let bytes = SoundBankCodec::encode_runtime(&bank).unwrap();
    for offset in [56, 60] {
        for frames in [0_u32, 2, 48_001, u32::MAX] {
            let mut changed = bytes.to_vec();
            changed[offset..offset + 4].copy_from_slice(&frames.to_le_bytes());
            assert_runtime_code(&changed, "game.sound-bank.runtime-length");
        }
    }
    let oversized = vec![0; SoundBankCodec::runtime_limits().encoded_bytes().get() + 1];
    assert_runtime_code(&oversized, "game.sound-bank.runtime-bounds");
}

#[test]
fn complete_product_membership_never_aliases_a_foreign_source_or_auxiliary() {
    let bank = prepare(PICKUP, TRAVEL).unwrap();
    let auxiliary_id = ImportedProductId::parse_subproduct(FOREIGN_SOURCE_ID).unwrap();
    let auxiliary = AssetProductRef::new(bank_product().source(), auxiliary_id).unwrap();
    let foreign = AssetProductRef::new(
        StableAssetId::parse_str(FOREIGN_SOURCE_ID).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap();
    assert!(SoundBankDomain::contains_product(
        &bank,
        ImportedProductId::PRIMARY
    ));
    assert!(!SoundBankDomain::contains_product(&bank, auxiliary_id));
    assert!(SoundBankCodec::contains_runtime_product(
        &bank,
        bank_product()
    ));
    assert!(!SoundBankCodec::contains_runtime_product(&bank, auxiliary));
    assert!(!SoundBankCodec::contains_runtime_product(&bank, foreign));
    for product in [auxiliary, foreign] {
        let invalid = SoundBank {
            product,
            pickup: Arc::from([0.0]),
            travel: Arc::from([0.0]),
        };
        let error = SoundBankCodec::encode_runtime(&invalid).unwrap_err();
        assert_eq!(
            error.diagnostic().code().as_str(),
            "game.sound-bank.runtime-identity"
        );
    }
}

#[test]
fn encoder_and_retention_queries_reject_noncanonical_samples_and_invalid_lengths() {
    for invalid in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -0.0,
        1.0,
        -1.5,
        0.1,
    ] {
        let bank = SoundBank {
            product: bank_product(),
            pickup: Arc::from([invalid]),
            travel: Arc::from([0.0]),
        };
        for error in [
            SoundBankCodec::encode_runtime(&bank).unwrap_err(),
            SoundBankCodec::runtime_retained_bytes(&bank).unwrap_err(),
        ] {
            assert_eq!(
                error.diagnostic().code().as_str(),
                "game.sound-bank.canonical-samples"
            );
        }
    }
    for frames in [0, MAX_CLIP_FRAMES + 1] {
        let bank = SoundBank {
            product: bank_product(),
            pickup: Arc::from(vec![0.0; frames]),
            travel: Arc::from([0.0]),
        };
        assert_eq!(
            SoundBankCodec::encode_runtime(&bank)
                .unwrap_err()
                .diagnostic()
                .code()
                .as_str(),
            "game.sound-bank.runtime-length",
        );
        assert!(SoundBankCodec::runtime_retained_bytes(&bank).is_err());
    }
}

#[test]
fn admitted_storage_and_maximum_wire_fit_original_reservations_including_arc_padding() {
    for (pickup, travel) in [(1, 3), (48_000, 48_000)] {
        let bank = prepare(&pcm_wav(&vec![0; pickup]), &pcm_wav(&vec![0; travel])).unwrap();
        let encoded = SoundBankCodec::encode_runtime(&bank).unwrap();
        let decoded = SoundBankCodec::decode_runtime(&encoded).unwrap();
        let allocation = |frames| {
            Layout::new::<[AtomicUsize; 2]>()
                .extend(Layout::array::<f32>(frames).unwrap())
                .unwrap()
                .0
                .pad_to_align()
                .size()
        };
        let expected = size_of::<SoundBank>() + allocation(pickup) + allocation(travel);
        let retained = SoundBankCodec::runtime_retained_bytes(&decoded).unwrap();
        assert_eq!(retained, expected);
        assert!(retained <= SoundBankCodec::runtime_limits().retained_bytes().get());
        assert!(retained <= SoundBankDomain::limits().retained_bytes().get());
        let encoded = SoundBankCodec::encode_runtime(&decoded).unwrap();
        assert_eq!(encoded.len(), 64 + 2 * (pickup + travel));
        if pickup == MAX_CLIP_FRAMES {
            assert_eq!(
                encoded.len(),
                SoundBankCodec::runtime_limits().encoded_bytes().get()
            );
        }
    }
}

#[test]
fn captured_samples_stay_exact_after_new_source_preparation_and_original_bank_drop() {
    let old = prepare(&pcm_wav(&[1, 2]), &pcm_wav(&[-1, -2])).unwrap();
    let (pickup, travel) = old.sample_arcs();
    let (second_pickup, second_travel) = old.sample_arcs();
    assert!(Arc::ptr_eq(&pickup, &second_pickup));
    assert!(Arc::ptr_eq(&travel, &second_travel));
    let identical = prepare(&pcm_wav(&[1, 2]), &pcm_wav(&[-1, -2])).unwrap();
    let replacement = prepare(&pcm_wav(&[3, 4]), &pcm_wav(&[-1, -2])).unwrap();
    assert_eq!(old, identical);
    assert_ne!(old, replacement);
    assert_eq!(old.product(), replacement.product());
    drop(old);
    assert_eq!(&*pickup, &[1.0 / 32_768.0, 2.0 / 32_768.0]);
    assert_eq!(&*travel, &[-1.0 / 32_768.0, -2.0 / 32_768.0]);
    assert_eq!(identical.samples(SoundCue::Pickup), &*pickup);
    assert_eq!(identical.samples(SoundCue::Travel), &*travel);
}
