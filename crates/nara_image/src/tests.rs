use super::*;
use image::ImageEncoder;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nara_app::App;
use nara_asset::{
    AssetEventKind, AssetEvents, AssetId, AssetLoadGeneration, AssetLoadGenerations, AssetPath,
    AssetRecord, AssetReloadDiagnostics, AssetReloadRequest, AssetReloadRequestKind,
    AssetReloadRequests, AssetServer, AssetSourceChangeKind, AssetSourceChanges, AssetSourceKind,
    AssetStates, AssetVersion, Assets, Handle, ImportArtifactPathError, ImportDependencyDigest,
    ImportProfile, ImportRequest, ImportSettingsHash, Importer, ImporterRegistry,
    ImporterSelectionError, LoadState, SourceExtension, SourceHash, StableAssetId,
};
use nara_core::{ByteLimit, ItemLimit};
use nara_diagnostic::{
    Diagnostic, DiagnosticFieldClass, DiagnosticReport, DiagnosticSeverity, DiagnosticValueRef,
};
use nara_fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, TrustMode};
use nara_render::{
    PreparedRenderResources, RenderPrepareInvalidationReason, RenderPrepareInvalidations,
};
use nara_tasks::{
    TaskCancellation, TaskCancellationReason, TaskCancellationToken, TaskCoalesceKey, TaskFailure,
    TaskHandle, TaskKindConfig, TaskOverloadPolicy, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskRejectReason, TaskRejection, TaskShutdownPolicy, TaskSpawnOutcome, TaskSpawnRequest,
};
use tracing::{Event, Metadata, Subscriber, field::Visit, span};

use crate::reload::{
    IMAGE_RELOAD_TASK_DOMAIN, ImageImportTaskResult, ImageReloadAttempt, PendingImageImportStream,
    PendingImageJobs, ReadyImageImportJob, apply_imported_image, image_reload_diagnostic,
    record_image_reload_failure,
};

fn stable_id() -> StableAssetId {
    StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap()
}

fn image_record(path: &str) -> AssetRecord {
    image_record_with_id(path, stable_id())
}

fn image_record_with_id(path: &str, stable_id: StableAssetId) -> AssetRecord {
    AssetRecord::new(
        stable_id,
        AssetPath::new(path).unwrap(),
        AssetSourceKind::Image,
    )
}

fn bounded_task_config(io_workers: usize, io_pending: usize) -> TaskPoolConfig {
    let one = ItemLimit::ONE;
    TaskPoolConfig::new(
        TaskKindConfig::new(
            ItemLimit::new(io_workers).unwrap(),
            ItemLimit::new(io_pending).unwrap(),
        ),
        TaskKindConfig::new(one, ItemLimit::new(4).unwrap()),
        TaskKindConfig::new(one, ItemLimit::new(4).unwrap()),
        TaskShutdownPolicy::default(),
    )
    .unwrap()
}

fn request<'a>(record: &'a AssetRecord, source_bytes: &'a [u8]) -> ImportRequest<'a> {
    ImportRequest::new(
        record,
        source_bytes,
        ImportDependencyDigest::empty(),
        ImportSettingsHash::default(),
        ImportProfile::default(),
    )
}

fn bytes_request(record: &AssetRecord, source_bytes: &[u8]) -> ImageBytesImportRequest {
    ImageBytesImportRequest::new(
        record.clone(),
        source_bytes.to_vec().into_boxed_slice(),
        ImportDependencyDigest::empty(),
        ImportSettingsHash::default(),
        ImportProfile::default(),
    )
}

fn try_import_uncommitted(
    importer: &ImageImporter,
    record: &AssetRecord,
    source_bytes: &[u8],
) -> Result<ImageImportedAsset, ImageImportError> {
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(record).unwrap();
    importer.import_image(
        bytes_request(record, source_bytes),
        handle,
        AssetVersion::ZERO,
        &server,
        &Assets::default(),
        &AssetStates::default(),
    )
}

fn import_uncommitted(
    importer: &ImageImporter,
    record: &AssetRecord,
    source_bytes: &[u8],
) -> ImageImportedAsset {
    try_import_uncommitted(importer, record, source_bytes).unwrap()
}

fn loaded_image_store(
    importer: &ImageImporter,
    record: &AssetRecord,
    pixels: &[u8],
) -> (
    AssetServer,
    Handle<ImageAsset>,
    Assets<ImageAsset>,
    AssetStates,
    AssetEvents,
) {
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(record).unwrap();
    let mut images = Assets::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let bytes = rgba_png(1, 1, pixels);
    importer
        .import_image(
            bytes_request(record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap()
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    events.drain();
    (server, handle, images, states, events)
}

fn rgba_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

const SAFE_DIAGNOSTIC_ASSET_PATH: &str = "textures/player.png";
const HOST_PATH_DETAIL_CANARY: &str = "C:\\private\\nara-host-path-canary.png";
const DECODER_DETAIL_CANARY: &str = "nara-decoder-detail-canary";
const PANIC_PAYLOAD_CANARY: &str = "nara-image-panic-payload-canary";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedDiagnosticValue {
    Identifier(&'static str),
    Unsigned(u64),
    Bool(bool),
    ProjectRelative(&'static str),
    Redacted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExpectedDiagnosticField {
    key: &'static str,
    class: DiagnosticFieldClass,
    value: ExpectedDiagnosticValue,
}

#[derive(Debug)]
struct ImageDiagnosticCase {
    name: String,
    path: AssetPath,
    error: ImageReloadError,
    code: &'static str,
    summary: &'static str,
    fields: Vec<ExpectedDiagnosticField>,
}

impl ImageDiagnosticCase {
    fn new(
        name: impl Into<String>,
        error: ImageReloadError,
        code: &'static str,
        summary: &'static str,
        fields: Vec<ExpectedDiagnosticField>,
    ) -> Self {
        let mut expected = Vec::with_capacity(fields.len() + 1);
        expected.push(project_path_field(SAFE_DIAGNOSTIC_ASSET_PATH));
        expected.extend(fields);
        Self {
            name: name.into(),
            path: AssetPath::new(SAFE_DIAGNOSTIC_ASSET_PATH).unwrap(),
            error,
            code,
            summary,
            fields: expected,
        }
    }
}

const fn identifier_field(key: &'static str, value: &'static str) -> ExpectedDiagnosticField {
    ExpectedDiagnosticField {
        key,
        class: DiagnosticFieldClass::Public,
        value: ExpectedDiagnosticValue::Identifier(value),
    }
}

const fn unsigned_field(key: &'static str, value: u64) -> ExpectedDiagnosticField {
    ExpectedDiagnosticField {
        key,
        class: DiagnosticFieldClass::Public,
        value: ExpectedDiagnosticValue::Unsigned(value),
    }
}

const fn bool_field(key: &'static str, value: bool) -> ExpectedDiagnosticField {
    ExpectedDiagnosticField {
        key,
        class: DiagnosticFieldClass::Public,
        value: ExpectedDiagnosticValue::Bool(value),
    }
}

const fn project_path_field(value: &'static str) -> ExpectedDiagnosticField {
    ExpectedDiagnosticField {
        key: "asset-path",
        class: DiagnosticFieldClass::ProjectRelative,
        value: ExpectedDiagnosticValue::ProjectRelative(value),
    }
}

const fn sensitive_field(key: &'static str) -> ExpectedDiagnosticField {
    ExpectedDiagnosticField {
        key,
        class: DiagnosticFieldClass::Sensitive,
        value: ExpectedDiagnosticValue::Redacted,
    }
}

#[test]
fn image_reload_diagnostic_matrix_preserves_codes_and_typed_privacy_fields() {
    let mut cases = vec![
        ImageDiagnosticCase::new(
            "missing-source-directory",
            ImageReloadError::MissingSourceDirectory,
            "image.reload-source-authority-missing",
            "Image source authority is absent",
            vec![],
        ),
        ImageDiagnosticCase::new(
            "import-selection",
            ImageReloadError::Import(ImageImportError::Selection(
                ImporterSelectionError::MissingSourceExtension {
                    path: AssetPath::new("textures/player").unwrap(),
                },
            )),
            "image.import-selection-failed",
            "Image importer selection failed",
            vec![identifier_field("stage", "admission")],
        ),
        ImageDiagnosticCase::new(
            "import-unsupported-format",
            ImageReloadError::Import(ImageImportError::UnsupportedFormat {
                extension: SourceExtension::new("gif").unwrap(),
            }),
            "image.import-format-unsupported",
            "Image format is unsupported",
            vec![identifier_field("stage", "admission")],
        ),
        ImageDiagnosticCase::new(
            "import-artifact-path",
            ImageReloadError::Import(ImageImportError::ArtifactPath(
                ImportArtifactPathError::OutsideImportCache {
                    path: HOST_PATH_DETAIL_CANARY.to_owned(),
                },
            )),
            "image.import-artifact-path-failed",
            "Image artifact path creation failed",
            vec![identifier_field("stage", "admission")],
        ),
        ImageDiagnosticCase::new(
            "task-failed",
            ImageReloadError::TaskFailed(TaskFailure::Panicked),
            "image.reload-task-failed",
            "Image import task failed",
            vec![
                identifier_field("reason", "panicked"),
                sensitive_field("panic-payload"),
            ],
        ),
        ImageDiagnosticCase::new(
            "task-tracking",
            ImageReloadError::TaskTracking,
            "image.reload-task-tracking-failed",
            "Image import task tracking failed",
            vec![],
        ),
    ];

    for stage in [
        ImageImportStage::Admission,
        ImageImportStage::SourceOpen,
        ImageImportStage::SourceRead,
        ImageImportStage::Header,
        ImageImportStage::Metadata,
        ImageImportStage::Decode,
        ImageImportStage::Finalize,
        ImageImportStage::Publication,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("import-cancelled-{}", stage.as_str()),
            ImageReloadError::Import(ImageImportError::Cancelled { stage }),
            "image.import-cancelled",
            "Image import was cancelled",
            vec![identifier_field("stage", stage.as_str())],
        ));
    }

    for feature in [
        ImageUnsupportedFeature::Animation,
        ImageUnsupportedFeature::Interlacing,
        ImageUnsupportedFeature::EmbeddedMetadata,
        ImageUnsupportedFeature::OutputColorModel,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("unsupported-feature-{}", feature.as_str()),
            ImageReloadError::Import(ImageImportError::Unsupported {
                stage: ImageImportStage::Metadata,
                feature,
            }),
            "image.import-feature-unsupported",
            "PNG feature is unsupported",
            vec![
                identifier_field("stage", "metadata"),
                identifier_field("feature", feature.as_str()),
            ],
        ));
    }

    for kind in [
        ImagePngFailureKind::Truncated,
        ImagePngFailureKind::InvalidData,
        ImagePngFailureKind::DecoderContract,
        ImagePngFailureKind::AllocationFailed,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("png-failure-{}", kind.as_str()),
            ImageReloadError::Import(ImageImportError::Png {
                stage: ImageImportStage::Decode,
                kind,
            }),
            "image.import-png-invalid",
            "PNG input is invalid",
            vec![
                identifier_field("stage", "decode"),
                identifier_field("reason", kind.as_str()),
                sensitive_field("decoder-detail"),
            ],
        ));
    }

    for kind in [
        ImageSourceFailureKind::InvalidLogicalPath,
        ImageSourceFailureKind::NotFound,
        ImageSourceFailureKind::PermissionDenied,
        ImageSourceFailureKind::AllocationFailed,
        ImageSourceFailureKind::Unsupported,
        ImageSourceFailureKind::AuthorityRejected,
        ImageSourceFailureKind::Io,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("source-failure-{}", kind.as_str()),
            ImageReloadError::Import(ImageImportError::Source {
                stage: ImageImportStage::SourceOpen,
                kind,
            }),
            "image.import-source-failed",
            "Image source access failed",
            vec![
                identifier_field("stage", "source-open"),
                identifier_field("reason", kind.as_str()),
                sensitive_field("source-detail"),
            ],
        ));
    }

    for kind in [
        ImagePublicationFailureKind::AlreadyLoaded,
        ImagePublicationFailureKind::SlotChanged,
        ImagePublicationFailureKind::TargetMismatch,
        ImagePublicationFailureKind::StateChanged,
        ImagePublicationFailureKind::ReloadValueMissing,
        ImagePublicationFailureKind::ReloadValueChanged,
        ImagePublicationFailureKind::UnknownAsset,
        ImagePublicationFailureKind::VersionExhausted,
        ImagePublicationFailureKind::Stale,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("import-publication-{}", kind.as_str()),
            ImageReloadError::Import(ImageImportError::Publication(kind)),
            "image.import-publication-invalid",
            "Image publication admission is invalid",
            vec![
                identifier_field("stage", "publication"),
                identifier_field("reason", kind.as_str()),
            ],
        ));
        cases.push(ImageDiagnosticCase::new(
            format!("reload-publication-{}", kind.as_str()),
            ImageReloadError::Publication(kind),
            "image.reload-publication-failed",
            "Image publication failed",
            vec![identifier_field("reason", kind.as_str())],
        ));
    }

    for reason in [
        TaskRejectReason::QueueFull { capacity: 7 },
        TaskRejectReason::PoolClosed,
        TaskRejectReason::TaskIdExhausted,
    ] {
        let reason_text = match reason {
            TaskRejectReason::QueueFull { .. } => "queue-full",
            TaskRejectReason::PoolClosed => "pool-closed",
            TaskRejectReason::TaskIdExhausted => "task-id-exhausted",
        };
        cases.push(ImageDiagnosticCase::new(
            format!("task-rejected-{reason_text}"),
            ImageReloadError::TaskRejected(TaskRejection {
                task: None,
                kind: TaskPoolKind::Io,
                admission_tick: 17,
                domain_key: IMAGE_RELOAD_TASK_DOMAIN,
                reason,
            }),
            "image.reload-task-rejected",
            "Image import task was rejected",
            vec![identifier_field("reason", reason_text)],
        ));
    }

    let task_pools = TaskPools::inline_for_tests(bounded_task_config(1, 2)).unwrap();
    let replacement_task = accepted_image_handle(task_pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(0, IMAGE_RELOAD_TASK_DOMAIN),
        |_| Err(ImageReloadError::TaskTracking),
    ));
    let replacement_task_id = replacement_task.id();
    assert!(replacement_task.cancel());
    let _ = task_pools.run_pending_for_tests();
    for (reason, reason_text, before_start) in [
        (TaskCancellationReason::Requested, "requested", true),
        (
            TaskCancellationReason::Coalesced {
                replacement: replacement_task_id,
            },
            "coalesced",
            false,
        ),
        (TaskCancellationReason::PoolShutdown, "pool-shutdown", true),
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("task-cancelled-{reason_text}"),
            ImageReloadError::TaskCancelled(TaskCancellation {
                reason,
                before_start,
            }),
            "image.reload-task-cancelled",
            "Image import task was cancelled",
            vec![
                identifier_field("reason", reason_text),
                bool_field("before-start", before_start),
            ],
        ));
    }

    for kind in [
        ImageImportLimitKind::EncodedBytes,
        ImageImportLimitKind::Width,
        ImageImportLimitKind::Height,
        ImageImportLimitKind::Pixels,
        ImageImportLimitKind::RgbaBytes,
        ImageImportLimitKind::DecoderWorkBytes,
        ImageImportLimitKind::AggregateInFlightBytes,
    ] {
        cases.push(ImageDiagnosticCase::new(
            format!("budget-{}-observed", kind.as_str()),
            ImageReloadError::Import(ImageImportError::Budget {
                stage: ImageImportStage::SourceRead,
                error: ImageImportBudgetError::per_image(kind, Some(13), 12),
            }),
            "image.import-budget-exceeded",
            "Image import budget was exceeded",
            vec![
                identifier_field("stage", "source-read"),
                identifier_field("limit-kind", kind.as_str()),
                unsigned_field("limit", 12),
                unsigned_field("observed", 13),
            ],
        ));
    }
    cases.push(ImageDiagnosticCase::new(
        "budget-observed-absent",
        ImageReloadError::Import(ImageImportError::Budget {
            stage: ImageImportStage::Header,
            error: ImageImportBudgetError::per_image(ImageImportLimitKind::Pixels, None, 12),
        }),
        "image.import-budget-exceeded",
        "Image import budget was exceeded",
        vec![
            identifier_field("stage", "header"),
            identifier_field("limit-kind", "pixels"),
            unsigned_field("limit", 12),
        ],
    ));
    cases.push(ImageDiagnosticCase::new(
        "budget-aggregate-in-use",
        ImageReloadError::Import(ImageImportError::Budget {
            stage: ImageImportStage::Admission,
            error: ImageImportBudgetError::aggregate(13, 7, 12),
        }),
        "image.import-budget-exceeded",
        "Image import budget was exceeded",
        vec![
            identifier_field("stage", "admission"),
            identifier_field("limit-kind", "aggregate-in-flight-bytes"),
            unsigned_field("limit", 12),
            unsigned_field("observed", 13),
            unsigned_field("in-use", 7),
        ],
    ));

    cases.push(ImageDiagnosticCase {
        name: "sensitive-asset-path-falls-back-to-redaction".to_owned(),
        path: AssetPath::new("textures/secret-token.png").unwrap(),
        error: ImageReloadError::MissingSourceDirectory,
        code: "image.reload-source-authority-missing",
        summary: "Image source authority is absent",
        fields: vec![sensitive_field("asset-path")],
    });

    for case in cases {
        assert_image_diagnostic_case(case);
    }
}

#[test]
fn image_reload_diagnostic_sinks_hide_upstream_host_and_decoder_detail_canaries() {
    let path = AssetPath::new(SAFE_DIAGNOSTIC_ASSET_PATH).unwrap();
    let host_path_error = ImageReloadError::Import(ImageImportError::ArtifactPath(
        ImportArtifactPathError::OutsideImportCache {
            path: HOST_PATH_DETAIL_CANARY.to_owned(),
        },
    ));
    assert!(matches!(
        &host_path_error,
        ImageReloadError::Import(ImageImportError::ArtifactPath(
            ImportArtifactPathError::OutsideImportCache { path }
        )) if path == HOST_PATH_DETAIL_CANARY
    ));
    let host_path_diagnostic = image_reload_diagnostic(&path, &host_path_error);
    assert_diagnostic_sinks_hide(&host_path_diagnostic, &[HOST_PATH_DETAIL_CANARY]);

    let record = image_record(SAFE_DIAGNOSTIC_ASSET_PATH);
    let mut malformed = rgba_png(1, 1, &[0, 0, 0, 255]);
    malformed[29] ^= 0xff;
    malformed.extend_from_slice(DECODER_DETAIL_CANARY.as_bytes());
    assert!(
        malformed
            .windows(DECODER_DETAIL_CANARY.len())
            .any(|window| window == DECODER_DETAIL_CANARY.as_bytes())
    );
    let decoder_error = try_import_uncommitted(&ImageImporter::default(), &record, &malformed)
        .expect_err("corrupted PNG must be rejected by the importer");
    assert!(matches!(decoder_error, ImageImportError::Png { .. }));
    let decoder_diagnostic =
        image_reload_diagnostic(&path, &ImageReloadError::Import(decoder_error));
    assert_eq!(
        diagnostic_field(&decoder_diagnostic, "decoder-detail"),
        "[REDACTED]"
    );
    assert_diagnostic_sinks_hide(&decoder_diagnostic, &[DECODER_DETAIL_CANARY]);
}

#[test]
fn png_import_produces_rgba8_image_asset() {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(
        2,
        1,
        &[
            255, 0, 0, 255, //
            0, 255, 0, 128,
        ],
    );

    let imported = import_uncommitted(&ImageImporter::default(), &record, &bytes);

    assert_eq!(imported.image().extent(), ImageExtent::new(2, 1));
    assert_eq!(imported.image().format(), ImageFormat::Rgba8);
    assert_eq!(imported.image().color_space(), ImageColorSpace::Srgb);
    assert_eq!(imported.image().pixels().len(), 8);
    assert_eq!(imported.image().source().stable_id(), stable_id());
    assert_eq!(
        imported.image().source().path().as_str(),
        "textures/player.png"
    );
    assert!(
        imported
            .artifact()
            .artifact_path()
            .as_str()
            .starts_with(".nara/import-cache/nara_image_png/default/nara_image.image/")
    );
}

#[test]
fn imported_candidate_debug_is_bounded_and_omits_pixel_values() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let imported = import_uncommitted(&importer, &record, &rgba_png(2, 2, &[173; 16]));

    let debug = format!("{imported:?}");
    let image_debug = format!("{:?}", imported.image());

    assert!(debug.contains("pixel_len: 16"));
    assert!(!debug.contains("173, 173"));
    assert!(debug.len() < 16 * 1024);
    assert!(image_debug.contains("pixel_len: 16"));
    assert!(!image_debug.contains("173, 173"));
    assert!(image_debug.len() < 16 * 1024);
    drop(imported);
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn unsupported_image_extension_returns_import_diagnostic() {
    let record = image_record("textures/player.jpg");
    let error =
        try_import_uncommitted(&ImageImporter::default(), &record, b"not a png").unwrap_err();

    assert!(matches!(
        error,
        ImageImportError::UnsupportedFormat { extension } if extension.as_str() == "jpg"
    ));
}

#[test]
fn importer_registry_selects_png_importer_by_extension() {
    let importer = ImageImporter::default();
    assert_eq!(
        nara_asset::Importer::descriptor(&importer).version(),
        nara_asset::ImporterVersion::new(2)
    );
    let mut registry = ImporterRegistry::new();
    registry.register(importer).unwrap();
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);

    let artifact = registry.import(request(&record, &bytes)).unwrap();

    assert_eq!(
        artifact.key().output_asset_type().as_str(),
        "nara_image.image"
    );
}

#[test]
fn admitted_file_import_owns_the_authorized_source() {
    let temp_root = unique_temp_root();
    let record = image_record("textures/player.png");
    let texture_path = temp_root.join(record.path().as_str());
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, rgba_png(1, 1, &[0, 0, 255, 255])).unwrap();
    let source_directory = image_source_directory(&temp_root);
    let file = source_directory.open(&record).unwrap();
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let images = Assets::default();
    let states = AssetStates::default();
    let admitted = ImageImporter::default()
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
        )
        .unwrap();

    let imported = admitted.import(TaskCancellationToken::new()).unwrap();

    assert_eq!(imported.image().extent(), ImageExtent::new(1, 1));
    assert_eq!(
        imported.artifact().key().output_asset_type().as_str(),
        "nara_image.image"
    );

    remove_temp_root(&temp_root);
}

fn imported_image_for_request(
    app: &App,
    importer: &ImageImporter,
    record: &AssetRecord,
    request: &AssetReloadRequest,
    pixels: &[u8],
) -> ImageImportedAsset {
    let bytes = rgba_png(1, 1, pixels);
    let world = app.world();
    importer
        .import_image(
            bytes_request(record, &bytes),
            Handle::new(request.asset_id()),
            request.expected_version(),
            world.resource::<AssetServer>(),
            world.resource::<Assets<ImageAsset>>(),
            world.resource::<AssetStates>(),
        )
        .unwrap()
}

#[test]
fn imported_image_stores_under_reserved_handle() {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let imported = ImageImporter::default()
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();

    imported
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();

    let stored = images.get(handle).unwrap();
    assert_eq!(stored.extent(), ImageExtent::new(1, 1));
    assert_eq!(server.stable_id(handle.id()), Some(stable_id()));
}

#[test]
fn publication_admission_rejects_a_mismatched_stable_target_without_charging() {
    let source = image_record_with_id(
        "textures/source.png",
        StableAssetId::parse_str("20ab890b-5b80-4ff7-aa0b-99702ecbf145").unwrap(),
    );
    let target = image_record_with_id(
        "textures/target.png",
        StableAssetId::parse_str("f6e87fe8-ac89-4c60-9423-1692b850bb1d").unwrap(),
    );
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&target).unwrap();

    let error = importer
        .import_image(
            bytes_request(&source, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &Assets::default(),
            &AssetStates::default(),
        )
        .unwrap_err();

    assert_eq!(
        error,
        ImageImportError::Publication(ImagePublicationFailureKind::TargetMismatch)
    );
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn publication_admission_rejects_an_existing_value_above_the_overlap_limit() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::with_limits(
        ImageImportLimits::default().with_max_rgba_bytes(ByteLimit::new(4).unwrap()),
    )
    .unwrap();
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let oversized =
        import_uncommitted(&ImageImporter::default(), &record, &rgba_png(2, 1, &[0; 8]))
            .into_image();
    let mut images = Assets::default();
    images.insert(handle, oversized);

    let error = importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[0; 4])),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &AssetStates::default(),
        )
        .unwrap_err();

    assert_eq!(
        error,
        ImageImportError::Budget {
            stage: ImageImportStage::Admission,
            error: ImageImportBudgetError::per_image(ImageImportLimitKind::RgbaBytes, Some(8), 4,),
        }
    );
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn first_load_candidate_rejects_late_slot_occupation_and_releases_charge() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let candidate = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let occupied = import_uncommitted(
        &ImageImporter::default(),
        &record,
        &rgba_png(1, 1, &[255, 0, 0, 255]),
    )
    .into_image();
    let occupied_hash = occupied.source().source_hash();
    assert!(images.insert(handle, occupied).is_none());

    let error = candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();

    assert_eq!(error, ImagePublicationFailureKind::AlreadyLoaded);
    assert_eq!(
        images.get(handle).unwrap().source().source_hash(),
        occupied_hash
    );
    assert!(states.state(handle.id()).is_none());
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn publication_candidate_rejects_state_change_and_releases_charge() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let candidate = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    states.set_loading(handle.id());
    images
        .record_load_failure(
            handle,
            &mut states,
            &mut events,
            "image.import-test-failure",
        )
        .unwrap();
    events.drain();

    let error = candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();

    assert_eq!(error, ImagePublicationFailureKind::StateChanged);
    assert!(matches!(
        states.state(handle.id()).unwrap().load_state(),
        LoadState::Failed { .. }
    ));
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn publication_candidate_rejects_same_version_state_aba() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let expected_version = states.set_loading(handle.id());
    let candidate = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    images
        .record_load_failure(
            handle,
            &mut states,
            &mut events,
            "image.import-test-failure",
        )
        .unwrap();
    assert_eq!(states.set_loading(handle.id()), expected_version);
    events.drain();

    let error = candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();

    assert_eq!(error, ImagePublicationFailureKind::StateChanged);
    assert!(images.get(handle).is_none());
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loading
    );
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn publication_candidate_cannot_cross_runtime_asset_stores() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let admitted_images = Assets::<ImageAsset>::default();
    let admitted_states = AssetStates::default();
    let candidate = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &admitted_images,
            &admitted_states,
        )
        .unwrap();
    let mut replacement_images = Assets::<ImageAsset>::default();
    let mut replacement_states = AssetStates::default();
    let mut events = AssetEvents::default();

    let error = candidate
        .commit(
            &server,
            &mut replacement_images,
            &mut replacement_states,
            &mut events,
        )
        .unwrap_err();

    assert_eq!(error, ImagePublicationFailureKind::StateChanged);
    assert!(replacement_images.get(handle).is_none());
    assert!(replacement_states.state(handle.id()).is_none());
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn first_load_candidate_rejects_empty_slot_aba() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let candidate = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let transient = import_uncommitted(
        &ImageImporter::default(),
        &record,
        &rgba_png(1, 1, &[255, 0, 0, 255]),
    )
    .into_image();
    images.insert(handle, transient);
    images.remove(handle).unwrap();

    let error = candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();

    assert_eq!(error, ImagePublicationFailureKind::SlotChanged);
    assert!(images.get(handle).is_none());
    assert!(states.state(handle.id()).is_none());
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn reload_candidate_rejects_missing_or_replaced_last_good_value() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let (server, handle, mut images, mut states, mut events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    let expected_version = states.set_loading(handle.id());
    let missing_candidate = importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[0, 0, 255, 255])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let last_good = images.remove(handle).unwrap();

    let missing_error = missing_candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();
    assert_eq!(
        missing_error,
        ImagePublicationFailureKind::ReloadValueMissing
    );
    assert!(events.drain().is_empty());
    images.insert(handle, last_good);

    let replaced_candidate = importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[0, 0, 255, 255])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let replacement = import_uncommitted(
        &ImageImporter::default(),
        &record,
        &rgba_png(1, 1, &[0, 255, 0, 255]),
    )
    .into_image();
    let replacement_hash = replacement.source().source_hash();
    images.insert(handle, replacement);

    let replaced_error = replaced_candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();
    assert_eq!(
        replaced_error,
        ImagePublicationFailureKind::ReloadValueChanged
    );
    assert_eq!(
        images.get(handle).unwrap().source().source_hash(),
        replacement_hash
    );
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn concurrent_candidates_retain_captured_replacement_overlap() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let (server, handle, mut images, mut states, mut events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    let expected_version = states.set_loading(handle.id());
    let first = importer
        .import_image(
            bytes_request(&record, &rgba_png(2, 2, &[10; 16])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let second = importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[20; 4])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let captured_overlap = images.get(handle).unwrap().pixels().len();
    assert_eq!(
        importer
            .budget_snapshot()
            .active_publication_overlap_bytes(),
        captured_overlap * 2
    );

    first
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    assert_eq!(images.get(handle).unwrap().extent(), ImageExtent::new(2, 2));
    assert_eq!(
        importer
            .budget_snapshot()
            .active_publication_overlap_bytes(),
        captured_overlap
    );
    let error = second
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap_err();
    assert_eq!(error, ImagePublicationFailureKind::Stale);
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn shared_host_freezes_one_overlap_ceiling_for_different_importer_limits() {
    let record = image_record("textures/player.png");
    let host_limits = ImageImportLimits::default().with_max_rgba_bytes(ByteLimit::new(16).unwrap());
    let host = ImageImportBudgetHost::new(host_limits).unwrap();
    let large = ImageImporter::with_budget_host(host_limits, host.clone()).unwrap();
    let small_limits = host_limits.with_max_rgba_bytes(ByteLimit::new(4).unwrap());
    let small = ImageImporter::with_budget_host(small_limits, host.clone()).unwrap();
    let (server, handle, mut images, mut states, mut events) =
        loaded_image_store(&large, &record, &[255, 0, 0, 255]);
    let captured_overlap = images.get(handle).unwrap().pixels().len();
    let expected_version = states.set_loading(handle.id());
    let large_candidate = large
        .import_image(
            bytes_request(&record, &rgba_png(2, 2, &[10; 16])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let small_candidate = small
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[20; 4])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    assert_eq!(
        small_candidate.memory_plan().publication_overlap_bytes(),
        captured_overlap
    );

    large_candidate
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    assert_eq!(
        host.snapshot().active_publication_overlap_bytes(),
        captured_overlap
    );
    assert_eq!(
        small_candidate
            .commit(&server, &mut images, &mut states, &mut events)
            .unwrap_err(),
        ImagePublicationFailureKind::Stale
    );
    assert_budget_released(host.snapshot());

    let too_large = host_limits.with_max_rgba_bytes(ByteLimit::new(32).unwrap());
    assert!(matches!(
        ImageImporter::with_budget_host(too_large, host),
        Err(ImageImporterCreateError::BudgetOverlapLimitTooSmall {
            required: 32,
            host: 16,
        })
    ));
}

#[test]
fn prepare_system_writes_backend_neutral_image_resource() {
    let (mut app, handle) = app_with_loaded_image(ImageImporter::default());

    app.update().unwrap();

    let prepared = app
        .world()
        .resource::<PreparedRenderResources<PreparedImageResource>>();
    let resource = prepared.get_ready(image_resource_key(handle)).unwrap();
    assert_eq!(resource.extent(), ImageExtent::new(1, 1));
    assert_eq!(resource.pixel_len(), 4);
    assert_eq!(app.world().resource::<ImagePrepareStats>().prepared, 1);
}

#[test]
fn prepare_system_invalidates_when_image_descriptor_changes() {
    let (mut app, handle) = app_with_loaded_image(ImageImporter::default());
    app.update().unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<RenderPrepareInvalidations>()
        .drain();
    let old_snapshot = app
        .world()
        .resource::<PreparedRenderResources<PreparedImageResource>>()
        .get(image_resource_key(handle))
        .unwrap()
        .snapshot();

    let changed_importer = ImageImporter::default().with_color_space(ImageColorSpace::Linear);
    let record = image_record("textures/player.png");
    let server = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetServer>()
        .unwrap();
    let mut images = app
        .world_mut()
        .unwrap()
        .remove_resource::<Assets<ImageAsset>>()
        .unwrap();
    let mut states = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetStates>()
        .unwrap();
    let expected_version = states.version(handle.id()).unwrap();
    let changed = changed_importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[0, 0, 255, 255])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    changed
        .commit(
            &server,
            &mut images,
            &mut states,
            &mut AssetEvents::default(),
        )
        .unwrap();
    app.world_mut().unwrap().insert_resource(server);
    app.world_mut().unwrap().insert_resource(images);
    app.world_mut().unwrap().insert_resource(states);

    app.update().unwrap();

    let prepared = app
        .world()
        .resource::<PreparedRenderResources<PreparedImageResource>>();
    let new_snapshot = prepared.get(image_resource_key(handle)).unwrap().snapshot();
    assert_ne!(old_snapshot, new_snapshot);
    assert_eq!(
        new_snapshot.asset_version().raw(),
        expected_version.raw() + 1
    );
    assert_eq!(app.world().resource::<ImagePrepareStats>().prepared, 1);
    assert!(
        app.world()
            .resource::<RenderPrepareInvalidations>()
            .iter()
            .any(
                |invalidation| invalidation.key() == image_resource_key(handle)
                    && invalidation.reason() == RenderPrepareInvalidationReason::DescriptorChanged
            )
    );
}

#[test]
fn prepare_system_removes_prepared_resources_for_removed_images() {
    let (mut app, handle) = app_with_loaded_image(ImageImporter::default());
    app.update().unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<RenderPrepareInvalidations>()
        .drain();
    assert!(
        app.world()
            .resource::<PreparedRenderResources<PreparedImageResource>>()
            .get_ready(image_resource_key(handle))
            .is_some()
    );

    let mut images = app
        .world_mut()
        .unwrap()
        .remove_resource::<Assets<ImageAsset>>()
        .unwrap();
    let mut states = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetStates>()
        .unwrap();
    images
        .remove_with_state(handle, &mut states, &mut AssetEvents::default())
        .unwrap();
    app.world_mut().unwrap().insert_resource(images);
    app.world_mut().unwrap().insert_resource(states);

    app.update().unwrap();

    assert!(
        app.world()
            .resource::<PreparedRenderResources<PreparedImageResource>>()
            .get_ready(image_resource_key(handle))
            .is_none()
    );
    assert_eq!(app.world().resource::<ImagePrepareStats>().removed, 1);
    assert!(
        app.world()
            .resource::<RenderPrepareInvalidations>()
            .iter()
            .any(
                |invalidation| invalidation.key() == image_resource_key(handle)
                    && invalidation.reason() == RenderPrepareInvalidationReason::AssetRemoved
            )
    );
}

#[test]
fn image_plugin_loads_and_reloads_image_through_task_update() {
    let temp_root = unique_temp_root();
    let texture_path = temp_root.join("textures").join("player.png");
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(&temp_root, record.clone());
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();

    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);

    let first_hash = app
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .source()
        .source_hash();
    assert_eq!(app.world().resource::<ImageReloadStats>().applied, 1);
    assert!(
        app.world()
            .resource::<PreparedRenderResources<PreparedImageResource>>()
            .get_ready(image_resource_key(handle))
            .is_some()
    );

    app.world_mut()
        .unwrap()
        .resource_mut::<RenderPrepareInvalidations>()
        .drain();
    fs::write(&texture_path, rgba_png(1, 1, &[0, 255, 0, 255])).unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);

    let image = app
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap();
    assert_ne!(image.source().source_hash(), first_hash);
    assert_eq!(app.world().resource::<ImageReloadStats>().applied, 2);
    assert_eq!(
        app.world()
            .resource::<ImageImporter>()
            .budget_snapshot()
            .high_water_publication_overlap_bytes(),
        image.pixels().len()
    );
    assert!(
        app.world()
            .resource::<RenderPrepareInvalidations>()
            .iter()
            .any(
                |invalidation| invalidation.key() == image_resource_key(handle)
                    && invalidation.reason() == RenderPrepareInvalidationReason::DescriptorChanged
            )
    );
    assert_app_image_budget_released(&app);

    remove_temp_root(&temp_root);
}

#[test]
fn image_plugin_records_first_load_failure_without_asset_value() {
    let temp_root = unique_temp_root();
    let texture_path = temp_root.join("textures").join("player.png");
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, b"not a png").unwrap();
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(&temp_root, record.clone());
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();

    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);

    assert!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { .. }
    ));
    assert!(
        app.world()
            .resource::<AssetEvents>()
            .iter()
            .any(|event| event.kind() == AssetEventKind::LoadFailed)
    );
    assert_app_image_budget_released(&app);

    remove_temp_root(&temp_root);
}

#[test]
fn budget_rejected_reload_preserves_last_good_value_and_asset_version() {
    let temp_root = unique_temp_root();
    let texture_path = temp_root.join("textures").join("player.png");
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    let valid = rgba_png(1, 1, &[255, 0, 0, 255]);
    fs::write(&texture_path, &valid).unwrap();
    let record = image_record("textures/player.png");
    let limits =
        ImageImportLimits::default().with_max_encoded_bytes(ByteLimit::new(valid.len()).unwrap());
    let mut app = app_with_image_plugin_configuration(
        &temp_root,
        [record.clone()],
        TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap(),
        ImagePlugin::with_limits(limits).unwrap(),
    );
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);

    let last_good_hash = app
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap()
        .source()
        .source_hash();
    let last_good_version = app
        .world()
        .resource::<AssetStates>()
        .version(handle.id())
        .unwrap();
    let successful_generation = app
        .world()
        .resource::<AssetLoadGenerations>()
        .current(handle.id());
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetReloadDiagnostics>()
        .clear();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetEvents>()
        .drain();

    let mut oversized = valid;
    oversized.push(0);
    fs::write(&texture_path, &oversized).unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);

    assert_eq!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash(),
        last_good_hash
    );
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .version(handle.id())
            .unwrap(),
        last_good_version
    );
    assert!(
        app.world()
            .resource::<AssetLoadGenerations>()
            .current(handle.id())
            > successful_generation
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { message } if message == "image.import-budget-exceeded"
    ));
    assert!(
        app.world()
            .resource::<AssetEvents>()
            .iter()
            .any(|event| event.kind() == AssetEventKind::ReloadFailed)
    );
    let diagnostic = app
        .world()
        .resource::<AssetReloadDiagnostics>()
        .iter()
        .next()
        .unwrap();
    assert_eq!(diagnostic.code().as_str(), "image.import-budget-exceeded");
    assert_eq!(diagnostic_field(diagnostic, "stage"), "source-read");
    assert_eq!(diagnostic_field(diagnostic, "limit-kind"), "encoded-bytes");
    assert_eq!(
        diagnostic_field(diagnostic, "limit"),
        limits.max_encoded_bytes().get().to_string()
    );
    assert_eq!(
        diagnostic_field(diagnostic, "observed"),
        oversized.len().to_string()
    );
    assert_app_image_budget_released(&app);

    remove_temp_root(&temp_root);
}

#[test]
fn ordered_image_stream_waits_for_earlier_task_before_reverse_completion_apply() {
    let record = image_record("textures/player.png");
    let task_pools = TaskPools::try_new(bounded_task_config(2, 8)).unwrap();
    let mut app = app_with_image_plugin_pools(Path::new("."), [record.clone()], task_pools);
    let (handle, request) = reserve_loading_request(&mut app, &record);
    let importer = ImageImporter::default();
    let first_imported =
        imported_image_for_request(&app, &importer, &record, &request, &[255, 0, 0, 255]);
    let first_hash = first_imported.image().source().source_hash();
    let second_imported =
        imported_image_for_request(&app, &importer, &record, &request, &[0, 255, 0, 255]);
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();

    let first = accepted_image_handle(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, IMAGE_RELOAD_TASK_DOMAIN),
        move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            Ok(first_imported)
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let second = accepted_image_handle(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, IMAGE_RELOAD_TASK_DOMAIN),
        move |_| Ok(second_imported),
    ));
    wait_for_task(&second);
    track_image_task(&mut app, request.clone(), first);
    track_image_task(&mut app, request, second);

    app.update().unwrap();

    assert!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
    );
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Loading
    );

    release_tx.send(()).unwrap();
    wait_for_completed_tasks(&app, 2);
    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash(),
        first_hash
    );
    assert_eq!(app.world().resource::<ImageReloadStats>().applied, 1);
    assert_eq!(app.world().resource::<ImageReloadStats>().stale, 1);
}

#[test]
fn image_ordering_is_per_asset_and_ready_snapshots_follow_task_order_keys() {
    let first_record = image_record_with_id(
        "textures/a.png",
        StableAssetId::parse_str("933965e8-6d5a-4a72-9bda-65af6bc52296").unwrap(),
    );
    let second_record = image_record("textures/b.png");
    let first_asset = AssetId::from_raw(20);
    let second_asset = AssetId::from_raw(10);
    let first_request = reload_request(&first_record, Handle::new(first_asset), AssetVersion::ZERO);
    let second_request = reload_request(
        &second_record,
        Handle::new(second_asset),
        AssetVersion::ZERO,
    );
    let mut pools = TaskPools::try_new(bounded_task_config(4, 8)).unwrap();
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (first_release_tx, first_release_rx) = mpsc::channel();

    let first = accepted_image_handle(pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(20, IMAGE_RELOAD_TASK_DOMAIN),
        move |_| {
            first_started_tx.send(()).unwrap();
            first_release_rx.recv().unwrap();
            Err(ImageReloadError::MissingSourceDirectory)
        },
    ));
    let first_key = first.order_key();
    first_started_rx
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    let first_later = accepted_image_handle(pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(20, IMAGE_RELOAD_TASK_DOMAIN),
        |_| Err(ImageReloadError::MissingSourceDirectory),
    ));
    let first_later_key = first_later.order_key();
    let unrelated = accepted_image_handle(pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(10, IMAGE_RELOAD_TASK_DOMAIN),
        move |_| Err(ImageReloadError::MissingSourceDirectory),
    ));
    let unrelated_key = unrelated.order_key();

    let mut pending = PendingImageJobs::default();
    pending
        .imports
        .entry(first_asset)
        .or_default()
        .push(detached_attempt(first_request.clone()), first)
        .unwrap();
    pending
        .imports
        .entry(first_asset)
        .or_default()
        .push(detached_attempt(first_request.clone()), first_later)
        .unwrap();
    pending
        .imports
        .entry(second_asset)
        .or_default()
        .push(detached_attempt(second_request.clone()), unrelated)
        .unwrap();

    wait_for_io_completions(&pools, 2);
    let first_snapshot = drain_ready_image_jobs(&mut pending);
    assert_eq!(
        first_snapshot
            .iter()
            .filter_map(|job| job.order_key)
            .collect::<Vec<_>>(),
        vec![unrelated_key],
        "an unrelated asset must not wait behind another asset's ordered prefix"
    );
    assert_eq!(pending.imports[&first_asset].len(), 2);

    let second_later = accepted_image_handle(pools.spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(30, IMAGE_RELOAD_TASK_DOMAIN),
        |_| Err(ImageReloadError::MissingSourceDirectory),
    ));
    let second_later_key = second_later.order_key();
    pending
        .imports
        .entry(second_asset)
        .or_default()
        .push(detached_attempt(second_request), second_later)
        .unwrap();
    wait_for_io_completions(&pools, 3);

    first_release_tx.send(()).unwrap();
    wait_for_io_completions(&pools, 4);
    let mut second_snapshot = drain_ready_image_jobs(&mut pending);
    assert_eq!(
        second_snapshot
            .iter()
            .filter_map(|job| job.order_key)
            .collect::<Vec<_>>(),
        vec![second_later_key, first_key, first_later_key],
        "the test must begin in AssetId order rather than task order"
    );
    second_snapshot.sort_by_key(ReadyImageImportJob::sort_key);
    assert_eq!(
        second_snapshot
            .iter()
            .filter_map(|job| job.order_key)
            .collect::<Vec<_>>(),
        vec![first_key, first_later_key, second_later_key]
    );
    assert!(
        pending
            .imports
            .values()
            .all(PendingImageImportStream::is_empty)
    );

    let _ = pools.shutdown_blocking();
}

#[test]
fn queue_rejection_records_failure_and_never_leaves_loading_state() {
    let temp_root = unique_temp_root();
    let first_record = image_record_with_id("textures/a.png", stable_id());
    let second_record = image_record_with_id(
        "textures/b.png",
        StableAssetId::parse_str("933965e8-6d5a-4a72-9bda-65af6bc52296").unwrap(),
    );
    for record in [&first_record, &second_record] {
        let path = temp_root.join(record.path().as_str());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, rgba_png(1, 1, &[255, 255, 255, 255])).unwrap();
    }
    let mut app = app_with_image_plugin_config(
        &temp_root,
        [first_record.clone(), second_record.clone()],
        bounded_task_config(1, 1),
    );
    let first_handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&first_record)
        .unwrap();
    let second_handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&second_record)
        .unwrap();
    {
        let mut changes = app
            .world_mut()
            .unwrap()
            .resource_mut::<AssetSourceChanges>();
        changes.modified(first_record.path().clone());
        changes.modified(second_record.path().clone());
    }

    app.update().unwrap();

    assert_eq!(app.world().resource::<ImageReloadStats>().rejected, 1);
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(first_handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Loading
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(second_handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { .. }
    ));

    let report = app.world().resource::<TaskPools>().run_pending_for_tests();
    assert_eq!(report.executed, 1);
    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(first_handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Loaded
    );
    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(second_handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { .. }
    ));
    assert_eq!(app.world().resource::<ImageReloadStats>().pending, 0);
    assert_app_image_budget_released(&app);

    remove_temp_root(&temp_root);
}

#[test]
fn panicked_image_reload_preserves_last_good_and_records_structured_failure() {
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(Path::new("."), record.clone());
    let (handle, request, last_good_hash) =
        reserve_loaded_image_request(&mut app, &record, &[255, 0, 0, 255]);
    let task = accepted_image_handle(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, IMAGE_RELOAD_TASK_DOMAIN),
        |_| -> ImageImportTaskResult { panic!("{PANIC_PAYLOAD_CANARY}") },
    ));
    track_image_task(&mut app, request, task);

    let report = app.world().resource::<TaskPools>().run_pending_for_tests();
    assert_eq!(report.executed, 1);
    app.update().unwrap();

    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { message } if message == "image.reload-task-failed"
    ));
    let diagnostic = app
        .world()
        .resource::<AssetReloadDiagnostics>()
        .iter()
        .next()
        .unwrap();
    assert_eq!(diagnostic.code().as_str(), "image.reload-task-failed");
    assert_eq!(diagnostic_field(diagnostic, "reason"), "panicked");
    assert_eq!(diagnostic_field(diagnostic, "panic-payload"), "[REDACTED]");
    assert_diagnostic_report_sinks_hide(
        app.world().resource::<AssetReloadDiagnostics>().report(),
        &[PANIC_PAYLOAD_CANARY],
    );
    assert_eq!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash(),
        last_good_hash
    );
    assert!(
        app.world()
            .resource::<AssetEvents>()
            .iter()
            .any(|event| event.kind() == AssetEventKind::ReloadFailed)
    );
}

#[test]
fn requested_task_cancellation_records_structured_load_failure() {
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(Path::new("."), record.clone());
    let (handle, request) = reserve_loading_request(&mut app, &record);
    let task = accepted_image_handle(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        TaskSpawnRequest::new(1, IMAGE_RELOAD_TASK_DOMAIN),
        |_| -> ImageImportTaskResult { unreachable!("cancelled pending task must not execute") },
    ));
    assert!(task.cancel());
    track_image_task(&mut app, request, task);

    app.update().unwrap();

    assert!(matches!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        LoadState::Failed { message } if message == "image.reload-task-cancelled"
    ));
    let diagnostic = app
        .world()
        .resource::<AssetReloadDiagnostics>()
        .iter()
        .next()
        .unwrap();
    assert_eq!(diagnostic.code().as_str(), "image.reload-task-cancelled");
    assert_eq!(diagnostic_field(diagnostic, "reason"), "requested");
    assert_eq!(diagnostic_field(diagnostic, "before-start"), "true");
    assert_eq!(app.world().resource::<ImageReloadStats>().cancelled, 1);
    let report = app.world().resource::<TaskPools>().run_pending_for_tests();
    assert_eq!(report.executed, 0);
}

#[test]
fn same_generation_coalesce_keeps_loading_until_replacement_is_ready() {
    let record = image_record("textures/player.png");
    let mut app =
        app_with_image_plugin_config(Path::new("."), [record.clone()], bounded_task_config(1, 2));
    let (handle, request) = reserve_loading_request(&mut app, &record);
    let importer = ImageImporter::default();
    let first_imported =
        imported_image_for_request(&app, &importer, &record, &request, &[255, 0, 0, 255]);
    let replacement_imported =
        imported_image_for_request(&app, &importer, &record, &request, &[0, 255, 0, 255]);
    let replacement_hash = replacement_imported.image().source().source_hash();
    let spawn_request = TaskSpawnRequest::new(1, IMAGE_RELOAD_TASK_DOMAIN).with_overload(
        TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(handle.id().raw())),
    );
    let first = accepted_image_handle(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        spawn_request,
        move |_| Ok(first_imported),
    ));
    track_image_task(&mut app, request.clone(), first);
    let replacement_outcome =
        app.world()
            .resource::<TaskPools>()
            .spawn(TaskPoolKind::Io, spawn_request, move |_| {
                Ok(replacement_imported)
            });
    assert!(matches!(
        &replacement_outcome,
        TaskSpawnOutcome::Coalesced { .. }
    ));
    let replacement = accepted_image_handle(replacement_outcome);
    track_image_task(&mut app, request, replacement);

    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Loading
    );
    assert_eq!(app.world().resource::<ImageReloadStats>().cancelled, 1);
    assert_eq!(app.world().resource::<ImageReloadStats>().failed, 0);
    assert!(
        !app.world()
            .resource::<AssetEvents>()
            .iter()
            .any(|event| event.kind() == AssetEventKind::LoadFailed)
    );

    let report = app.world().resource::<TaskPools>().run_pending_for_tests();
    assert_eq!(report.executed, 1);
    app.update().unwrap();

    assert_eq!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .unwrap()
            .source()
            .source_hash(),
        replacement_hash
    );
    assert_eq!(app.world().resource::<ImageReloadStats>().applied, 1);
    assert_eq!(app.world().resource::<ImageReloadStats>().pending, 0);
}

#[test]
fn stale_first_load_success_cannot_recreate_removed_image() {
    let record = image_record("textures/player.png");
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let expected_version = states.set_loading(handle.id());
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let imported = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    images
        .remove_with_state(handle, &mut states, &mut events)
        .unwrap();
    events.drain();
    let mut stats = ImageReloadStats::default();

    let error = apply_imported_image(
        imported,
        &server,
        &mut images,
        &mut states,
        &mut events,
        &mut stats,
    );

    assert_eq!(error, None);
    assert_eq!(stats.stale, 1);
    assert!(images.get(handle).is_none());
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Removed
    );
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn slot_conflict_is_stale_and_does_not_mark_the_winning_value_failed() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let (server, handle, mut images, mut states, mut events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    let expected_version = states.set_loading(handle.id());
    let candidate = importer
        .import_image(
            bytes_request(&record, &rgba_png(1, 1, &[0, 0, 255, 255])),
            handle,
            expected_version,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let winner = import_uncommitted(
        &ImageImporter::default(),
        &record,
        &rgba_png(1, 1, &[0, 255, 0, 255]),
    )
    .into_image();
    let winner_hash = winner.source().source_hash();
    images.insert(handle, winner);
    let mut stats = ImageReloadStats::default();

    let failure = apply_imported_image(
        candidate,
        &server,
        &mut images,
        &mut states,
        &mut events,
        &mut stats,
    );

    assert_eq!(failure, None);
    assert_eq!(stats.stale, 1);
    assert_eq!(stats.failed, 0);
    assert_eq!(
        images.get(handle).unwrap().source().source_hash(),
        winner_hash
    );
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loading
    );
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn publication_failure_releases_candidate_charge_for_the_failure_recorder() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let imported = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    assert_eq!(
        importer
            .budget_snapshot()
            .active_publication_overlap_bytes(),
        0
    );
    let request = reload_request(&record, handle, AssetVersion::ZERO);
    let attempt = ImageReloadAttempt::capture(request, &images, &states);
    let mut stats = ImageReloadStats::default();
    let unknown_server = AssetServer::new();

    let error = apply_imported_image(
        imported,
        &unknown_server,
        &mut images,
        &mut states,
        &mut events,
        &mut stats,
    );

    assert_eq!(
        error,
        Some(ImageReloadError::Publication(
            ImagePublicationFailureKind::UnknownAsset
        ))
    );
    assert_eq!(stats.failed, 0);
    assert!(images.get(handle).is_none());
    assert_eq!(
        importer
            .budget_snapshot()
            .high_water_publication_overlap_bytes(),
        0
    );
    assert_budget_released(importer.budget_snapshot());

    let mut diagnostics = AssetReloadDiagnostics::default();
    record_image_reload_failure(
        attempt,
        error.unwrap(),
        &mut images,
        &mut states,
        &mut events,
        &mut diagnostics,
        &mut stats,
    );
    assert_eq!(stats.failed, 1);
    assert_eq!(diagnostics.iter().count(), 1);
    assert_eq!(
        diagnostic_field(diagnostics.iter().next().unwrap(), "reason"),
        "unknown-asset"
    );
}

#[test]
fn stale_first_load_failure_cannot_overwrite_newer_state() {
    let record = image_record("textures/player.png");
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let expected_version = states.set_loading(handle.id());
    let request = reload_request(&record, handle, expected_version);
    let attempt = ImageReloadAttempt::capture(request, &images, &states);
    images
        .remove_with_state(handle, &mut states, &mut events)
        .unwrap();
    events.drain();
    let mut diagnostics = AssetReloadDiagnostics::default();
    let mut stats = ImageReloadStats::default();

    record_image_reload_failure(
        attempt,
        ImageReloadError::MissingSourceDirectory,
        &mut images,
        &mut states,
        &mut events,
        &mut diagnostics,
        &mut stats,
    );

    assert_eq!(stats.stale, 1);
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Removed
    );
    assert!(events.drain().is_empty());
}

#[test]
fn same_version_state_aba_retires_an_old_failure_as_stale() {
    let record = image_record("textures/player.png");
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let expected_version = states.set_loading(handle.id());
    let attempt = ImageReloadAttempt::capture(
        reload_request(&record, handle, expected_version),
        &images,
        &states,
    );
    images
        .record_load_failure(handle, &mut states, &mut events, "image.newer-failure")
        .unwrap();
    events.drain();
    assert_eq!(states.set_loading(handle.id()), expected_version);
    let mut diagnostics = AssetReloadDiagnostics::default();
    let mut stats = ImageReloadStats::default();

    record_image_reload_failure(
        attempt,
        ImageReloadError::MissingSourceDirectory,
        &mut images,
        &mut states,
        &mut events,
        &mut diagnostics,
        &mut stats,
    );

    assert_eq!(stats.stale, 1);
    assert_eq!(stats.failed, 0);
    assert!(diagnostics.is_empty());
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loading
    );
    assert!(events.drain().is_empty());
}

#[test]
fn slot_replacement_retires_an_old_failure_as_stale() {
    let record = image_record("textures/player.png");
    let importer = ImageImporter::default();
    let (_server, handle, mut images, mut states, mut events) =
        loaded_image_store(&importer, &record, &[255, 0, 0, 255]);
    let expected_version = states.set_loading(handle.id());
    let attempt = ImageReloadAttempt::capture(
        reload_request(&record, handle, expected_version),
        &images,
        &states,
    );
    let winner = import_uncommitted(
        &ImageImporter::default(),
        &record,
        &rgba_png(1, 1, &[0, 255, 0, 255]),
    )
    .into_image();
    let winner_hash = winner.source().source_hash();
    images.insert(handle, winner);
    let mut diagnostics = AssetReloadDiagnostics::default();
    let mut stats = ImageReloadStats::default();

    record_image_reload_failure(
        attempt,
        ImageReloadError::MissingSourceDirectory,
        &mut images,
        &mut states,
        &mut events,
        &mut diagnostics,
        &mut stats,
    );

    assert_eq!(stats.stale, 1);
    assert_eq!(stats.failed, 0);
    assert!(diagnostics.is_empty());
    assert_eq!(
        images.get(handle).unwrap().source().source_hash(),
        winner_hash
    );
    assert_eq!(
        states.state(handle.id()).unwrap().load_state(),
        &LoadState::Loading
    );
    assert!(events.drain().is_empty());
    assert_budget_released(importer.budget_snapshot());
}

#[test]
fn image_plugin_removes_runtime_and_prepared_image() {
    let temp_root = unique_temp_root();
    let texture_path = temp_root.join("textures").join("player.png");
    fs::create_dir_all(texture_path.parent().unwrap()).unwrap();
    fs::write(&texture_path, rgba_png(1, 1, &[255, 0, 0, 255])).unwrap();
    let record = image_record("textures/player.png");
    let mut app = app_with_image_plugin(&temp_root, record.clone());
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone());
    drive_image_jobs(&mut app);
    app.world_mut()
        .unwrap()
        .resource_mut::<RenderPrepareInvalidations>()
        .drain();

    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .removed(record.path().clone());
    app.update().unwrap();

    assert!(
        app.world()
            .resource::<Assets<ImageAsset>>()
            .get(handle)
            .is_none()
    );
    assert_eq!(
        app.world()
            .resource::<AssetStates>()
            .state(handle.id())
            .unwrap()
            .load_state(),
        &LoadState::Removed
    );
    assert!(
        app.world()
            .resource::<PreparedRenderResources<PreparedImageResource>>()
            .get_ready(image_resource_key(handle))
            .is_none()
    );
    assert!(
        app.world()
            .resource::<RenderPrepareInvalidations>()
            .iter()
            .any(
                |invalidation| invalidation.key() == image_resource_key(handle)
                    && invalidation.reason() == RenderPrepareInvalidationReason::AssetRemoved
            )
    );

    remove_temp_root(&temp_root);
}

#[test]
fn descriptor_hash_changes_when_content_descriptor_changes() {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let image = import_uncommitted(&ImageImporter::default(), &record, &bytes).into_image();
    let changed = import_uncommitted(
        &ImageImporter::default().with_color_space(ImageColorSpace::Linear),
        &record,
        &bytes,
    )
    .into_image();

    assert_ne!(
        image_descriptor_hash(&image),
        image_descriptor_hash(&changed)
    );
}

#[test]
fn image_asset_reuses_the_admitted_pixel_allocation() {
    let record = image_record("textures/player.png");
    let source_bytes = b"image allocation fixture";
    let artifact = ImageImporter::default()
        .import(request(&record, source_bytes))
        .unwrap();
    let pixels = vec![24, 120, 220, 255];
    let admitted_allocation = pixels.as_ptr();

    let image = ImageAsset::new(
        ImageSourceMetadata::new(
            record.stable_id(),
            record.path().clone(),
            SourceHash::from_bytes(source_bytes),
            artifact,
        ),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Srgb,
        pixels,
    );

    assert_eq!(image.pixels().as_ptr(), admitted_allocation);
}

#[cfg(feature = "serde")]
#[test]
fn shared_image_pixels_keep_the_canonical_byte_content_wire_shape() {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[24, 120, 220, 255]);
    let image = import_uncommitted(&ImageImporter::default(), &record, &bytes).into_image();

    let encoded = serde_json::to_value(&image).unwrap();
    assert_eq!(
        encoded.get("pixels"),
        Some(&serde_json::json!([24, 120, 220, 255])),
    );

    let decoded = serde_json::from_value::<ImageAsset>(encoded).unwrap();
    assert_eq!(decoded, image);
    assert_eq!(decoded.pixels(), &[24, 120, 220, 255]);
    assert!(!std::ptr::eq(decoded.pixels(), image.pixels()));
}

#[test]
fn unpublished_byte_preflight_matches_the_import_memory_plan() {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[24, 120, 220, 255]);
    let importer = ImageImporter::default();

    let preflight = importer
        .preflight_unpublished_import(bytes_request(&record, &bytes))
        .unwrap();
    let memory_plan = preflight.memory_plan();
    let image = preflight.import().unwrap();
    let unpublished_budget = importer.budget_snapshot();
    assert_eq!(unpublished_budget.active_reservations(), 0);
    assert_eq!(
        unpublished_budget.high_water_bytes(),
        memory_plan.peak_bytes()
    );
    let imported = import_uncommitted(&importer, &record, &bytes);

    assert_eq!(memory_plan, imported.memory_plan());
    assert_eq!(memory_plan.publication_overlap_bytes(), 0);
    assert_eq!(image, *imported.image());
}

fn app_with_loaded_image(importer: ImageImporter) -> (App, Handle<ImageAsset>) {
    let record = image_record("textures/player.png");
    let bytes = rgba_png(1, 1, &[0, 0, 255, 255]);
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let imported = importer
        .import_image(
            bytes_request(&record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    imported
        .commit(
            &server,
            &mut images,
            &mut states,
            &mut AssetEvents::default(),
        )
        .unwrap();
    let mut app = App::new();
    app.add_plugin(nara_reflect::ComponentRegistryPlugin)
        .unwrap();
    app.add_plugin(nara_render::RenderPlugin).unwrap();
    app.add_plugin(ImagePreparePlugin).unwrap();
    app.world_mut().unwrap().insert_resource(server);
    app.world_mut().unwrap().insert_resource(images);
    app.world_mut().unwrap().insert_resource(states);
    (app, handle)
}

fn app_with_image_plugin(asset_root: &Path, record: AssetRecord) -> App {
    app_with_image_plugin_config(asset_root, [record], TaskPoolConfig::default())
}

#[test]
fn image_plugin_definition_binds_canonical_limits_and_prepares_that_configuration() {
    let limits =
        ImageImportLimits::default().with_max_width(std::num::NonZeroU32::new(2_048).unwrap());
    let first = crate::plugin(limits);
    let repeated = crate::plugin(limits);
    let changed = crate::plugin(ImageImportLimits::default());

    assert_eq!(first.key(), repeated.key());
    assert_ne!(first.key(), changed.key());

    let plan = nara_app::PluginPlan::resolve((
        nara_app::PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
        nara_app::PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
        nara_app::PluginDefinition::for_default::<ImagePreparePlugin>(),
        first,
    ))
    .unwrap();
    let app = plan.instantiate().unwrap();

    assert_eq!(app.world().resource::<ImageImporter>().limits(), limits);
}

#[test]
fn invalid_image_plugin_limits_fail_during_preparation() {
    let limits = ImageImportLimits::default().with_max_in_flight_bytes(ByteLimit::new(1).unwrap());
    let plan = nara_app::PluginPlan::resolve((
        nara_app::PluginDefinition::for_default::<nara_tasks::TaskPlugin>(),
        nara_app::PluginDefinition::for_default::<nara_asset::AssetPlugin>(),
        nara_app::PluginDefinition::for_default::<ImagePreparePlugin>(),
        crate::plugin(limits),
    ))
    .unwrap();

    assert!(matches!(
        plan.instantiate(),
        Err(nara_app::PluginInstantiationError::Prepare(
            nara_app::PluginPrepareError::Failed {
                plugin: IMAGE_PLUGIN_ID,
                code: "nara.image.invalid-import-limits",
            }
        ))
    ));
}

fn app_with_image_plugin_config(
    asset_root: &Path,
    records: impl IntoIterator<Item = AssetRecord>,
    task_config: TaskPoolConfig,
) -> App {
    app_with_image_plugin_pools(
        asset_root,
        records,
        TaskPools::inline_for_tests(task_config).unwrap(),
    )
}

fn app_with_image_plugin_pools(
    asset_root: &Path,
    records: impl IntoIterator<Item = AssetRecord>,
    task_pools: TaskPools,
) -> App {
    app_with_image_plugin_configuration(asset_root, records, task_pools, ImagePlugin::default())
}

fn app_with_image_plugin_configuration(
    asset_root: &Path,
    records: impl IntoIterator<Item = AssetRecord>,
    task_pools: TaskPools,
    plugin: ImagePlugin,
) -> App {
    let mut app = App::new();
    app.insert_resource(task_pools).unwrap();
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_tasks::TaskPlugin::default(),
        nara_asset::AssetPlugin,
        nara_render::RenderPlugin,
        ImagePreparePlugin,
        plugin.with_source_directory(image_source_directory(asset_root)),
    ))
    .unwrap();
    {
        let mut database = app
            .world_mut()
            .unwrap()
            .resource_mut::<nara_asset::ProjectAssetDatabase>();
        for record in records {
            database.insert(record).unwrap();
        }
    }
    app
}

fn image_source_directory(path: &Path) -> ImageSourceDirectory {
    let directory = DirectoryCapability::from_host_handle(
        host_directory(path),
        HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
    )
    .unwrap();
    ImageSourceDirectory::new(directory)
}

fn host_directory(path: &Path) -> fs::File {
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
        fs::File::open(path).unwrap()
    }
}

fn drive_image_jobs(app: &mut App) {
    app.update().unwrap();
    let _ = app.world().resource::<TaskPools>().run_pending_for_tests();
    app.update().unwrap();
}

fn accepted_image_handle(
    outcome: TaskSpawnOutcome<ImageImportTaskResult>,
) -> TaskHandle<ImageImportTaskResult> {
    match outcome {
        TaskSpawnOutcome::Accepted(handle) | TaskSpawnOutcome::Coalesced { handle, .. } => handle,
        TaskSpawnOutcome::Rejected(rejection) => {
            panic!("expected accepted image task, got {rejection:?}")
        }
    }
}

fn track_image_task(
    app: &mut App,
    request: AssetReloadRequest,
    handle: TaskHandle<ImageImportTaskResult>,
) {
    let asset_id = request.asset_id();
    let attempt = ImageReloadAttempt::capture(
        request,
        app.world().resource::<Assets<ImageAsset>>(),
        app.world().resource::<AssetStates>(),
    );
    let tracked = app
        .world_mut()
        .unwrap()
        .resource_mut::<PendingImageJobs>()
        .imports
        .entry(asset_id)
        .or_default()
        .push(attempt, handle);
    assert!(tracked.is_ok());
}

fn detached_attempt(request: AssetReloadRequest) -> ImageReloadAttempt {
    ImageReloadAttempt::capture(
        request,
        &Assets::<ImageAsset>::default(),
        &AssetStates::default(),
    )
}

fn reserve_loading_request(
    app: &mut App,
    record: &AssetRecord,
) -> (Handle<ImageAsset>, AssetReloadRequest) {
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(record)
        .unwrap();
    let expected_version = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetStates>()
        .set_loading(handle.id());
    (handle, reload_request(record, handle, expected_version))
}

fn reserve_loaded_image_request(
    app: &mut App,
    record: &AssetRecord,
    pixels: &[u8],
) -> (Handle<ImageAsset>, AssetReloadRequest, SourceHash) {
    let mut server = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetServer>()
        .unwrap();
    let handle = server.reserve_record::<ImageAsset>(record).unwrap();
    let mut images = app
        .world_mut()
        .unwrap()
        .remove_resource::<Assets<ImageAsset>>()
        .unwrap();
    let mut states = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetStates>()
        .unwrap();
    let mut events = app
        .world_mut()
        .unwrap()
        .remove_resource::<AssetEvents>()
        .unwrap();
    let bytes = rgba_png(1, 1, pixels);
    let imported = ImageImporter::default()
        .import_image(
            bytes_request(record, &bytes),
            handle,
            AssetVersion::ZERO,
            &server,
            &images,
            &states,
        )
        .unwrap();
    let last_good_hash = imported.image().source().source_hash();
    imported
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    events.drain();
    let expected_version = states.set_loading(handle.id());
    app.world_mut().unwrap().insert_resource(server);
    app.world_mut().unwrap().insert_resource(images);
    app.world_mut().unwrap().insert_resource(states);
    app.world_mut().unwrap().insert_resource(events);
    (
        handle,
        reload_request(record, handle, expected_version),
        last_good_hash,
    )
}

fn wait_for_completed_tasks(app: &App, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app
        .world()
        .resource::<TaskPools>()
        .stats()
        .for_kind(TaskPoolKind::Io)
        .completed
        < expected
        && Instant::now() < deadline
    {
        thread::yield_now();
    }
    assert!(
        app.world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .completed
            >= expected,
        "image tasks did not complete before the test deadline"
    );
}

fn wait_for_task<T>(handle: &TaskHandle<T>) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        thread::yield_now();
    }
    assert!(
        handle.is_finished(),
        "task did not finish before the test deadline"
    );
}

fn wait_for_io_completions(pools: &TaskPools, expected: u64) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while pools.stats().for_kind(TaskPoolKind::Io).completed < expected && Instant::now() < deadline
    {
        thread::yield_now();
    }
    assert!(
        pools.stats().for_kind(TaskPoolKind::Io).completed >= expected,
        "image tasks did not complete before the test deadline"
    );
}

fn drain_ready_image_jobs(pending: &mut PendingImageJobs) -> Vec<ReadyImageImportJob> {
    pending
        .imports
        .values_mut()
        .flat_map(PendingImageImportStream::drain_ready_prefix)
        .collect()
}

fn assert_image_diagnostic_case(case: ImageDiagnosticCase) {
    let diagnostic = image_reload_diagnostic(&case.path, &case.error);
    assert_eq!(
        diagnostic.code().as_str(),
        case.code,
        "unexpected code for {}",
        case.name
    );
    assert_eq!(
        diagnostic.summary().as_str(),
        case.summary,
        "unexpected summary for {}",
        case.name
    );
    assert_eq!(
        diagnostic.severity(),
        DiagnosticSeverity::Error,
        "unexpected severity for {}",
        case.name
    );
    assert_eq!(
        diagnostic.fields().len(),
        case.fields.len(),
        "unexpected field count for {}: {diagnostic:?}",
        case.name
    );

    for expected in case.fields {
        let field = diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == expected.key)
            .unwrap_or_else(|| {
                panic!(
                    "missing diagnostic field {:?} for {}: {diagnostic:?}",
                    expected.key, case.name
                )
            });
        assert_eq!(
            field.class(),
            expected.class,
            "unexpected class for {}.{}",
            case.name,
            expected.key
        );
        assert_eq!(
            field.value(),
            expected_value_ref(expected.value),
            "unexpected value for {}.{}",
            case.name,
            expected.key
        );
    }
}

const fn expected_value_ref(value: ExpectedDiagnosticValue) -> DiagnosticValueRef<'static> {
    match value {
        ExpectedDiagnosticValue::Identifier(value) => DiagnosticValueRef::Identifier(value),
        ExpectedDiagnosticValue::Unsigned(value) => DiagnosticValueRef::Unsigned(value),
        ExpectedDiagnosticValue::Bool(value) => DiagnosticValueRef::Bool(value),
        ExpectedDiagnosticValue::ProjectRelative(value) => {
            DiagnosticValueRef::ProjectRelative(value)
        }
        ExpectedDiagnosticValue::Redacted => DiagnosticValueRef::Redacted,
    }
}

fn assert_diagnostic_sinks_hide(diagnostic: &Diagnostic, hidden: &[&str]) {
    let debug = format!("{diagnostic:?}");
    let tracing = capture_tracing(|| diagnostic.emit_to_tracing());
    assert_values_hidden("Diagnostic Debug", &debug, hidden);
    assert_values_hidden("Diagnostic tracing", &tracing, hidden);

    #[cfg(feature = "serde")]
    {
        let serialized = serde_json::to_string(diagnostic).unwrap();
        assert_values_hidden("Diagnostic serialization", &serialized, hidden);
    }
}

fn assert_diagnostic_report_sinks_hide(report: &DiagnosticReport, hidden: &[&str]) {
    let debug = format!("{report:?}");
    let tracing = capture_tracing(|| report.emit_to_tracing());
    assert_values_hidden("DiagnosticReport Debug", &debug, hidden);
    assert_values_hidden("DiagnosticReport tracing", &tracing, hidden);

    #[cfg(feature = "serde")]
    {
        let serialized = serde_json::to_string(report).unwrap();
        assert_values_hidden("DiagnosticReport serialization", &serialized, hidden);
    }
}

fn assert_values_hidden(sink: &str, output: &str, hidden: &[&str]) {
    for value in hidden {
        assert!(!output.contains(value), "{sink} leaked {value:?}: {output}");
    }
}

fn capture_tracing(emit: impl FnOnce()) -> String {
    let captured = Arc::new(Mutex::new(Vec::new()));
    let subscriber = RecordingSubscriber::new(Arc::clone(&captured));
    tracing::subscriber::with_default(subscriber, emit);
    let output = captured.lock().unwrap().join("\n");
    output
}

#[derive(Clone)]
struct RecordingSubscriber {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingSubscriber {
    fn new(events: Arc<Mutex<Vec<String>>>) -> Self {
        Self { events }
    }
}

#[derive(Default)]
struct FieldRecorder {
    output: String,
}

impl Visit for FieldRecorder {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        use fmt::Write as _;
        let _ = write!(self.output, "{}={value:?};", field.name());
    }
}

impl Subscriber for RecordingSubscriber {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}

    fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut recorder = FieldRecorder::default();
        event.record(&mut recorder);
        self.events.lock().unwrap().push(recorder.output);
    }

    fn enter(&self, _span: &span::Id) {}

    fn exit(&self, _span: &span::Id) {}
}

fn diagnostic_field(diagnostic: &Diagnostic, key: &str) -> String {
    diagnostic
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)
        .unwrap_or_else(|| panic!("missing diagnostic field {key}"))
        .display_value()
        .into_owned()
}

fn assert_app_image_budget_released(app: &App) {
    assert_budget_released(app.world().resource::<ImageImporter>().budget_snapshot());
}

fn assert_budget_released(snapshot: ImageImportBudgetSnapshot) {
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

fn reload_request(
    record: &AssetRecord,
    handle: Handle<ImageAsset>,
    expected_version: AssetVersion,
) -> AssetReloadRequest {
    let mut requests = AssetReloadRequests::new();
    requests.push_resolved(
        handle.id(),
        record,
        AssetReloadRequestKind::LoadOrReload,
        AssetSourceChangeKind::Modified,
        expected_version,
        AssetLoadGeneration::ZERO,
        Vec::new(),
    );
    requests
        .drain_for_source_kind(&AssetSourceKind::Image)
        .into_iter()
        .next()
        .unwrap()
}

fn unique_temp_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nara_image_test_{}_{}", std::process::id(), stamp))
}

fn remove_temp_root(path: &Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        panic!(
            "failed to remove temp test directory {}: {error}",
            path.display()
        );
    }
}
