use std::{
    fs::{self, File},
    num::{NonZeroU32, NonZeroU64},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_asset::{
    AssetPath, AssetRecord, AssetServer, AssetSourceKind, AssetStates, AssetVersion, Assets,
    ImportDependencyDigest, ImportProfile, ImportSettingsHash, StableAssetId,
};
use nara_core::ByteLimit;
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_image::{
    AdmittedImageImport, ImageBytesImportRequest, ImageFileImportRequest, ImageImportBudgetHost,
    ImageImportBudgetSnapshot, ImageImportError, ImageImportLimitKind, ImageImportLimits,
    ImageImportStage, ImageImportedAsset, ImageImporter, ImagePngFailureKind, ImageSourceDirectory,
    ImageUnsupportedFeature,
};
use nara_tasks::{
    TaskCancellationToken, TaskDomainKey, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskSpawnOutcome, TaskSpawnRequest, TaskTerminal,
};

const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const EXIF_PNG: &[u8] = include_bytes!("../../../tests/fixtures/images/exif-metadata.png");
const OVERSIZED_EXIF_PAYLOAD_BYTES: usize = 8 * 1024 * 1024 + 1;

#[test]
fn file_read_accepts_exact_encoded_limit_and_rejects_one_sentinel_byte() {
    let root = TempRoot::new();
    let record = image_record();
    let bytes = rgba_png(1, 1, &[10, 20, 30, 255]);
    root.write(record.path().as_str(), &bytes);
    let source = source_directory(root.path());
    let limits =
        ImageImportLimits::default().with_max_encoded_bytes(ByteLimit::new(bytes.len()).unwrap());
    let importer = ImageImporter::with_limits(limits).unwrap();

    let imported = import_file(&importer, &source, record.clone()).unwrap();
    let plan = imported.memory_plan();
    assert_eq!(plan.encoded_bytes(), bytes.len());
    let publication = importer.budget_snapshot();
    assert_eq!(publication.active_encoded_bytes(), 0);
    assert_eq!(publication.active_decoder_work_bytes(), 0);
    assert_eq!(publication.active_rgba_bytes(), 4);
    assert_eq!(publication.high_water_encoded_bytes(), bytes.len());
    assert_eq!(
        publication.high_water_decoder_work_bytes(),
        plan.decoder_work_bytes()
    );
    drop(imported);

    let mut oversized = bytes;
    oversized.push(0);
    root.write(record.path().as_str(), &oversized);
    let error = import_file(&importer, &source, record).unwrap_err();
    assert_budget_error(
        error,
        ImageImportStage::SourceRead,
        ImageImportLimitKind::EncodedBytes,
        Some(oversized.len() as u64),
    );
    assert_budget_fully_released(importer.budget_snapshot());
}

#[test]
fn real_ihdr_enforces_dimension_pixel_and_rgba_boundaries_before_decode() {
    let record = image_record();
    let valid = rgba_png(1, 1, &[10, 20, 30, 255]);
    let exact_limits = ImageImportLimits::default()
        .with_max_width(NonZeroU32::new(1).unwrap())
        .with_max_height(NonZeroU32::new(1).unwrap())
        .with_max_pixels(NonZeroU64::new(1).unwrap())
        .with_max_rgba_bytes(ByteLimit::new(4).unwrap());
    let exact = ImageImporter::with_limits(exact_limits).unwrap();
    drop(import_image(&exact, &record, &valid).unwrap());

    let width = patch_ihdr(&valid, 2, 1, 0);
    assert_budget_error(
        import_image(&exact, &record, &width).unwrap_err(),
        ImageImportStage::Header,
        ImageImportLimitKind::Width,
        Some(2),
    );
    let height = patch_ihdr(&valid, 1, 2, 0);
    assert_budget_error(
        import_image(&exact, &record, &height).unwrap_err(),
        ImageImportStage::Header,
        ImageImportLimitKind::Height,
        Some(2),
    );

    let pixel_limited = ImageImporter::with_limits(
        exact_limits
            .with_max_width(NonZeroU32::new(2).unwrap())
            .with_max_pixels(NonZeroU64::new(1).unwrap()),
    )
    .unwrap();
    assert_budget_error(
        import_image(&pixel_limited, &record, &width).unwrap_err(),
        ImageImportStage::Header,
        ImageImportLimitKind::Pixels,
        Some(2),
    );

    let rgba_limited = ImageImporter::with_limits(
        exact_limits
            .with_max_width(NonZeroU32::new(2).unwrap())
            .with_max_pixels(NonZeroU64::new(2).unwrap())
            .with_max_rgba_bytes(ByteLimit::new(7).unwrap()),
    )
    .unwrap();
    assert_budget_error(
        import_image(&rgba_limited, &record, &width).unwrap_err(),
        ImageImportStage::Header,
        ImageImportLimitKind::RgbaBytes,
        Some(8),
    );
}

#[test]
fn rgb_and_grayscale_rows_expand_to_rgba_without_a_native_frame_copy() {
    let record = image_record();
    let importer = ImageImporter::default();
    let rgb = encoded_png(2, 1, png::ColorType::Rgb, &[1, 2, 3, 4, 5, 6], false);
    let gray = encoded_png(2, 1, png::ColorType::Grayscale, &[7, 9], false);

    let rgb = import_image(&importer, &record, &rgb).unwrap();
    assert_eq!(rgb.image().pixels(), &[1, 2, 3, 255, 4, 5, 6, 255]);
    drop(rgb);
    let gray = import_image(&importer, &record, &gray).unwrap();
    assert_eq!(gray.image().pixels(), &[7, 7, 7, 255, 9, 9, 9, 255]);
    drop(gray);

    assert_budget_fully_released(importer.budget_snapshot());
}

#[test]
fn animation_and_adam7_are_rejected_by_the_audited_static_png_contract() {
    let record = image_record();
    let importer = ImageImporter::default();
    let animated = encoded_png(1, 1, png::ColorType::Rgba, &[1, 2, 3, 255], true);
    let interlaced = patch_ihdr(&rgba_png(1, 1, &[1, 2, 3, 255]), 1, 1, 1);

    assert!(matches!(
        import_image(&importer, &record, &animated).unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Metadata,
            feature: ImageUnsupportedFeature::Animation,
        }
    ));
    assert!(matches!(
        import_image(&importer, &record, &interlaced).unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Header,
            feature: ImageUnsupportedFeature::Interlacing,
        }
    ));
    assert_budget_fully_released(importer.budget_snapshot());
}

#[test]
fn corrupt_truncated_and_compressed_amplification_inputs_fail_without_charge_leaks() {
    let record = image_record();
    let importer = ImageImporter::default();
    let valid = rgba_png(1, 1, &[1, 2, 3, 255]);
    let mut corrupt_crc = valid.clone();
    corrupt_crc[29] ^= 0x01;
    let mut truncated = valid;
    truncated.truncate(truncated.len() - 6);

    assert!(matches!(
        import_image(&importer, &record, &corrupt_crc).unwrap_err(),
        ImageImportError::Png {
            kind: ImagePngFailureKind::InvalidData,
            ..
        }
    ));
    assert!(matches!(
        import_image(&importer, &record, &truncated).unwrap_err(),
        ImageImportError::Png {
            kind: ImagePngFailureKind::InvalidData | ImagePngFailureKind::Truncated,
            ..
        }
    ));

    let pixels = vec![0_u8; 256 * 256];
    let compressed = encoded_png(256, 256, png::ColorType::Grayscale, &pixels, false);
    assert!(compressed.len() < pixels.len());
    let amplification_limited = ImageImporter::with_limits(
        ImageImportLimits::default().with_max_rgba_bytes(ByteLimit::new(64 * 1024).unwrap()),
    )
    .unwrap();
    assert_budget_error(
        import_image(&amplification_limited, &record, &compressed).unwrap_err(),
        ImageImportStage::Header,
        ImageImportLimitKind::RgbaBytes,
        Some((256 * 256 * 4) as u64),
    );
    assert_budget_fully_released(importer.budget_snapshot());
    assert_budget_fully_released(amplification_limited.budget_snapshot());
}

#[test]
fn oversized_exif_chunk_is_rejected_before_decoder_allocation() {
    let record = image_record();
    let importer = ImageImporter::default();
    let valid = rgba_png(1, 1, &[1, 2, 3, 255]);
    let hostile = png_with_oversized_exif_chunk(&valid);

    assert_complete_oversized_exif_png(&hostile);

    assert!(matches!(
        import_image(&importer, &record, &hostile).unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Metadata,
            feature: ImageUnsupportedFeature::EmbeddedMetadata,
        }
    ));
    let snapshot = importer.budget_snapshot();
    assert_eq!(snapshot.high_water_encoded_bytes(), hostile.len());
    assert_eq!(snapshot.high_water_decoder_work_bytes(), 0);
    assert_eq!(snapshot.high_water_rgba_bytes(), 0);
    assert_budget_fully_released(snapshot);
}

#[test]
fn exif_metadata_is_rejected_before_the_decoder_can_clone_it() {
    let record = image_record();
    let importer = ImageImporter::default();

    assert!(matches!(
        import_image(&importer, &record, EXIF_PNG).unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Metadata,
            feature: ImageUnsupportedFeature::EmbeddedMetadata,
        }
    ));
    assert_eq!(
        importer.budget_snapshot().high_water_decoder_work_bytes(),
        0
    );
    assert_budget_fully_released(importer.budget_snapshot());
}

#[test]
fn decoder_work_and_complete_peak_accept_exact_budget_and_reject_limit_plus_one() {
    let record = image_record();
    let bytes = rgba_png(2, 2, &[0_u8; 16]);
    let probe = import_image(&ImageImporter::default(), &record, &bytes).unwrap();
    let plan = probe.memory_plan();
    drop(probe);

    const MIB: usize = 1024 * 1024;
    const PNG_V1_TRACKED_DECODER_BYTES: usize = 8 * MIB;
    const PNG_V1_UNTRACKED_BASE_BYTES: usize = 512 * 1024;
    const PNG_V1_RGBA_BYTES: usize = 2 * 2 * 4;
    const PNG_V1_ROW_SLACK_BYTES: usize = 2 * 4 * 3;
    const PNG_V1_DECODER_WORK_BYTES: usize =
        PNG_V1_TRACKED_DECODER_BYTES + PNG_V1_UNTRACKED_BASE_BYTES + PNG_V1_ROW_SLACK_BYTES;

    assert_eq!(plan.version(), 1);
    assert_eq!(plan.encoded_bytes(), bytes.len());
    assert_eq!(plan.encoded_allocation_bytes(), bytes.len());
    assert_eq!(plan.rgba_bytes(), PNG_V1_RGBA_BYTES);
    assert_eq!(plan.decoder_work_bytes(), PNG_V1_DECODER_WORK_BYTES);
    assert_eq!(plan.publication_overlap_bytes(), 0);
    assert_eq!(
        plan.peak_bytes(),
        bytes.len() + PNG_V1_DECODER_WORK_BYTES + PNG_V1_RGBA_BYTES
    );

    let base = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(bytes.len()).unwrap())
        .with_max_rgba_bytes(ByteLimit::new(plan.rgba_bytes()).unwrap())
        .with_max_decoder_work_bytes(ByteLimit::new(plan.decoder_work_bytes()).unwrap());
    let exact = ImageImporter::with_limits(
        base.with_max_in_flight_bytes(ByteLimit::new(plan.peak_bytes()).unwrap()),
    )
    .unwrap();
    let imported = import_image(&exact, &record, &bytes).unwrap();
    assert_eq!(
        exact.budget_snapshot().high_water_bytes(),
        plan.peak_bytes()
    );
    assert_eq!(
        exact.budget_snapshot().high_water_decoder_work_bytes(),
        plan.decoder_work_bytes()
    );
    drop(imported);
    assert_budget_fully_released(exact.budget_snapshot());

    let work_below = ImageImporter::with_limits(
        base.with_max_decoder_work_bytes(ByteLimit::new(plan.decoder_work_bytes() - 1).unwrap()),
    )
    .unwrap();
    assert_budget_error(
        import_image(&work_below, &record, &bytes).unwrap_err(),
        ImageImportStage::Metadata,
        ImageImportLimitKind::DecoderWorkBytes,
        Some(plan.decoder_work_bytes() as u64),
    );

    let aggregate_below = ImageImporter::with_limits(
        base.with_max_in_flight_bytes(ByteLimit::new(plan.peak_bytes() - 1).unwrap()),
    )
    .unwrap();
    assert_budget_error(
        import_image(&aggregate_below, &record, &bytes).unwrap_err(),
        ImageImportStage::Metadata,
        ImageImportLimitKind::AggregateInFlightBytes,
        Some(plan.peak_bytes() as u64),
    );
    assert_budget_fully_released(work_below.budget_snapshot());
    assert_budget_fully_released(aggregate_below.budget_snapshot());
}

#[test]
fn explicit_budget_host_coordinates_complete_peaks_across_importers() {
    let record = image_record();
    let bytes = rgba_png(1, 1, &[1, 2, 3, 255]);
    let probe = import_image(&ImageImporter::default(), &record, &bytes).unwrap();
    let plan = probe.memory_plan();
    drop(probe);
    let shared_limit = plan.peak_bytes() + plan.rgba_bytes() - 1;
    let limits = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(bytes.len()).unwrap())
        .with_max_rgba_bytes(ByteLimit::new(plan.rgba_bytes()).unwrap())
        .with_max_in_flight_bytes(ByteLimit::new(shared_limit).unwrap());
    let host = ImageImportBudgetHost::new(limits).unwrap();
    let first = ImageImporter::with_budget_host(limits, host.clone()).unwrap();
    let second = ImageImporter::with_budget_host(limits, host.clone()).unwrap();

    let first_candidate = import_image(&first, &record, &bytes).unwrap();
    assert_eq!(
        second.budget_snapshot().active_rgba_bytes(),
        plan.rgba_bytes()
    );
    assert_eq!(
        second.budget_snapshot().active_publication_overlap_bytes(),
        0
    );
    assert_budget_error(
        import_image(&second, &record, &bytes).unwrap_err(),
        ImageImportStage::Metadata,
        ImageImportLimitKind::AggregateInFlightBytes,
        Some(plan.peak_bytes() as u64),
    );

    drop(first_candidate);
    drop(import_image(&second, &record, &bytes).unwrap());
    assert_budget_fully_released(host.snapshot());
}

#[test]
fn pending_and_running_cancellation_release_admission_charge_exactly_once() {
    let root = TempRoot::new();
    let record = image_record();
    root.write(record.path().as_str(), &rgba_png(1, 1, &[1, 2, 3, 255]));
    let source = source_directory(root.path());
    let importer = ImageImporter::default();

    let pending = admit_file(&importer, &source, record.clone()).unwrap();
    let mut inline = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut pending_handle = accepted_task(inline.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, TaskDomainKey::new(1)),
        move |cancellation| pending.import(cancellation),
    ));
    assert!(pending_handle.cancel());
    assert_eq!(inline.run_pending_for_tests().executed, 0);
    assert!(matches!(
        pending_handle.try_take(),
        Some(TaskTerminal::Cancelled(cancellation)) if cancellation.before_start
    ));
    assert_budget_fully_released(importer.budget_snapshot());
    let _ = inline.shutdown_blocking();

    let running = admit_file(&importer, &source, record).unwrap();
    let mut threaded = TaskPools::try_new(TaskPoolConfig::default()).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut running_handle = accepted_task(threaded.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(2, TaskDomainKey::new(1)),
        move |cancellation| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            running.import(cancellation)
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(running_handle.cancel());
    release_tx.send(()).unwrap();
    assert!(matches!(
        running_handle.try_take(),
        Some(TaskTerminal::Cancelled(cancellation)) if !cancellation.before_start
    ));
    wait_for_budget_release(&importer);
    let _ = threaded.shutdown_blocking();
    assert_budget_fully_released(importer.budget_snapshot());
}

#[test]
fn panic_and_pool_rejection_drop_unpublished_admission_charge() {
    let root = TempRoot::new();
    let record = image_record();
    root.write(record.path().as_str(), &rgba_png(1, 1, &[1, 2, 3, 255]));
    let source = source_directory(root.path());
    let importer = ImageImporter::default();

    let panicking = admit_file(&importer, &source, record.clone()).unwrap();
    let mut inline = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let mut handle = accepted_task(inline.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, TaskDomainKey::new(2)),
        move |_| -> Result<ImageImportedAsset, ImageImportError> {
            let _reservation_owner = panicking;
            panic!("intentional image import panic")
        },
    ));
    assert_eq!(inline.run_pending_for_tests().executed, 1);
    assert!(matches!(handle.try_take(), Some(TaskTerminal::Failed(_))));
    assert_budget_fully_released(importer.budget_snapshot());
    let _ = inline.shutdown_blocking();

    let rejected = admit_file(&importer, &source, record).unwrap();
    let mut closed = TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap();
    let _ = closed.shutdown_blocking();
    let outcome = closed.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(2, TaskDomainKey::new(2)),
        move |cancellation| rejected.import(cancellation),
    );
    assert!(matches!(outcome, TaskSpawnOutcome::Rejected(_)));
    assert_budget_fully_released(importer.budget_snapshot());
}

fn import_file(
    importer: &ImageImporter,
    source: &ImageSourceDirectory,
    record: AssetRecord,
) -> Result<nara_image::ImageImportedAsset, ImageImportError> {
    let file = source.open(&record)?;
    let mut server = AssetServer::new();
    let handle = server
        .reserve_record::<nara_image::ImageAsset>(&record)
        .unwrap();
    let images = Assets::default();
    let states = AssetStates::default();
    importer
        .admit_file(
            ImageFileImportRequest::new(
                record,
                file,
                ImportDependencyDigest::empty(),
                ImportSettingsHash::default(),
                ImportProfile::default(),
            ),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )?
        .import(TaskCancellationToken::new())
}

fn admit_file(
    importer: &ImageImporter,
    source: &ImageSourceDirectory,
    record: AssetRecord,
) -> Result<AdmittedImageImport, ImageImportError> {
    let file = source.open(&record)?;
    let mut server = AssetServer::new();
    let handle = server
        .reserve_record::<nara_image::ImageAsset>(&record)
        .unwrap();
    importer.admit_file(
        ImageFileImportRequest::new(
            record,
            file,
            ImportDependencyDigest::empty(),
            ImportSettingsHash::default(),
            ImportProfile::default(),
        ),
        handle,
        AssetVersion::ZERO,
        &server,
        &Assets::default(),
        &AssetStates::default(),
    )
}

fn accepted_task<T>(outcome: TaskSpawnOutcome<T>) -> nara_tasks::TaskHandle<T> {
    match outcome {
        TaskSpawnOutcome::Accepted(handle) | TaskSpawnOutcome::Coalesced { handle, .. } => handle,
        TaskSpawnOutcome::Rejected(rejection) => panic!("task was rejected: {rejection:?}"),
    }
}

fn wait_for_budget_release(importer: &ImageImporter) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while importer.budget_snapshot().active_reservations() != 0 && Instant::now() < deadline {
        thread::yield_now();
    }
    assert_eq!(importer.budget_snapshot().active_reservations(), 0);
}

fn image_record() -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(TEXTURE_ID).unwrap(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    )
}

fn import_request(record: &AssetRecord, bytes: &[u8]) -> ImageBytesImportRequest {
    ImageBytesImportRequest::new(
        record.clone(),
        bytes.to_vec().into_boxed_slice(),
        ImportDependencyDigest::empty(),
        ImportSettingsHash::default(),
        ImportProfile::default(),
    )
}

fn import_image(
    importer: &ImageImporter,
    record: &AssetRecord,
    bytes: &[u8],
) -> Result<ImageImportedAsset, ImageImportError> {
    let mut server = AssetServer::new();
    let handle = server
        .reserve_record::<nara_image::ImageAsset>(record)
        .unwrap();
    importer.import_image(
        import_request(record, bytes),
        handle,
        AssetVersion::ZERO,
        &server,
        &Assets::default(),
        &AssetStates::default(),
    )
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    encoded_png(width, height, png::ColorType::Rgba, pixels, false)
}

fn encoded_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    pixels: &[u8],
    animated: bool,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    if animated {
        encoder.set_animated(1, 0).unwrap();
    }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    drop(writer);
    bytes
}

fn patch_ihdr(source: &[u8], width: u32, height: u32, interlace: u8) -> Vec<u8> {
    let mut bytes = source.to_vec();
    assert_eq!(&bytes[12..16], b"IHDR");
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    bytes[28] = interlace;
    let checksum = crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&checksum.to_be_bytes());
    bytes
}

fn png_with_oversized_exif_chunk(source: &[u8]) -> Vec<u8> {
    const IHDR_END: usize = 33;
    const MINIMAL_TIFF_PROFILE: &[u8] = b"II*\0\x08\0\0\0\0\0\0\0\0\0";

    assert!(source.len() > IHDR_END);
    assert_eq!(&source[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&source[12..16], b"IHDR");

    let chunk_bytes = OVERSIZED_EXIF_PAYLOAD_BYTES.checked_add(12).unwrap();
    let mut bytes = Vec::with_capacity(source.len().checked_add(chunk_bytes).unwrap());
    bytes.extend_from_slice(&source[..IHDR_END]);
    bytes.extend_from_slice(
        &u32::try_from(OVERSIZED_EXIF_PAYLOAD_BYTES)
            .unwrap()
            .to_be_bytes(),
    );
    let chunk_type_offset = bytes.len();
    bytes.extend_from_slice(b"eXIf");
    let payload_offset = bytes.len();
    bytes.resize(payload_offset + OVERSIZED_EXIF_PAYLOAD_BYTES, 0);
    bytes[payload_offset..payload_offset + MINIMAL_TIFF_PROFILE.len()]
        .copy_from_slice(MINIMAL_TIFF_PROFILE);
    let checksum = crc32(&bytes[chunk_type_offset..]);
    bytes.extend_from_slice(&checksum.to_be_bytes());
    bytes.extend_from_slice(&source[IHDR_END..]);
    bytes
}

fn assert_complete_oversized_exif_png(bytes: &[u8]) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut offset = 8_usize;
    let mut chunk_index = 0_usize;
    let mut saw_exif = false;
    let mut saw_idat = false;
    let mut saw_iend = false;

    while offset < bytes.len() {
        let header_end = offset.checked_add(8).unwrap();
        let header = bytes
            .get(offset..header_end)
            .expect("complete chunk header");
        let length = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let kind = &header[4..8];
        let data_end = header_end.checked_add(length).unwrap();
        let chunk_end = data_end.checked_add(4).unwrap();
        let chunk = bytes
            .get(offset..chunk_end)
            .expect("complete chunk payload and CRC");
        let expected_crc = u32::from_be_bytes(chunk[chunk.len() - 4..].try_into().unwrap());
        assert_eq!(crc32(&chunk[4..chunk.len() - 4]), expected_crc);

        match kind {
            b"IHDR" => {
                assert_eq!(chunk_index, 0);
                assert_eq!(length, 13);
            }
            b"eXIf" => {
                assert_eq!(chunk_index, 1);
                assert_eq!(length, OVERSIZED_EXIF_PAYLOAD_BYTES);
                assert!(!saw_exif);
                assert!(!saw_idat);
                saw_exif = true;
            }
            b"IDAT" => {
                assert!(saw_exif);
                assert!(!saw_iend);
                saw_idat = true;
            }
            b"IEND" => {
                assert_eq!(length, 0);
                assert!(saw_idat);
                assert!(!saw_iend);
                assert_eq!(chunk_end, bytes.len());
                saw_iend = true;
            }
            _ => panic!("unexpected PNG chunk {kind:?}"),
        }

        offset = chunk_end;
        chunk_index += 1;
    }

    assert!(saw_exif);
    assert!(saw_idat);
    assert!(saw_iend);
    assert_eq!(offset, bytes.len());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut checksum = u32::MAX;
    for byte in bytes {
        checksum ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(checksum & 1);
            checksum = (checksum >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !checksum
}

fn assert_budget_error(
    error: ImageImportError,
    stage: ImageImportStage,
    kind: ImageImportLimitKind,
    observed: Option<u64>,
) {
    let ImageImportError::Budget {
        stage: actual_stage,
        error,
    } = error
    else {
        panic!("expected budget error, got {error:?}");
    };
    assert_eq!(error.kind(), kind);
    assert_eq!(error.observed(), observed);
    assert_eq!(actual_stage, stage);
}

fn assert_budget_fully_released(snapshot: ImageImportBudgetSnapshot) {
    assert_eq!(snapshot.active_reservations(), 0);
    assert_eq!(snapshot.active_bytes(), 0);
    assert_eq!(snapshot.active_encoded_bytes(), 0);
    assert_eq!(snapshot.active_decoder_work_bytes(), 0);
    assert_eq!(snapshot.active_rgba_bytes(), 0);
    assert_eq!(snapshot.active_publication_overlap_bytes(), 0);
    assert_eq!(
        snapshot.total_reserved_bytes(),
        snapshot.total_released_bytes()
    );
    assert_eq!(
        snapshot.accepted_reservations(),
        snapshot.released_reservations()
    );
}

fn source_directory(path: &Path) -> ImageSourceDirectory {
    let directory = DirectoryCapability::from_host_handle(
        host_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory)
}

fn host_directory(path: &Path) -> File {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    {
        File::open(path).unwrap()
    }
}

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara_image_limits_{}_{}",
            std::process::id(),
            stamp
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let temp = std::env::temp_dir();
        assert!(self.0.starts_with(&temp));
        assert_ne!(self.0, temp);
        fs::remove_dir_all(&self.0).unwrap();
    }
}
