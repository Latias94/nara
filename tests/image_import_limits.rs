#![cfg(feature = "runtime-2d")]

use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nara::advanced_prelude::*;
use nara::image::ImageSourceDirectory;

const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const VALID_PNG: &[u8] = include_bytes!("fixtures/images/valid-rgba-1x1.png");
const EXIF_PNG: &[u8] = include_bytes!("fixtures/images/exif-metadata.png");
const ENCODED_PLUS_ONE_PNG: &[u8] = include_bytes!("fixtures/images/encoded-limit-plus-one.png");
const INTERLACED_PNG: &[u8] = include_bytes!("fixtures/images/adam7-interlaced.png");
const WIDTH_PLUS_ONE_PNG: &[u8] = include_bytes!("fixtures/images/width-limit-plus-one.png");
const INVALID_CRC_PNG: &[u8] = include_bytes!("fixtures/images/invalid-ihdr-crc.png");
const TRUNCATED_PNG: &[u8] = include_bytes!("fixtures/images/truncated.png");
const OVERSIZED_ANCILLARY_PNG: &[u8] =
    include_bytes!("fixtures/images/oversized-ancillary-chunk.png");

#[test]
fn root_facade_exposes_configured_bounded_png_import() {
    let record = image_record();
    let context = ImagePublicationContext::new(&record);
    let limits = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(VALID_PNG.len()).unwrap());
    let importer = ImageImporter::with_limits(limits).unwrap();

    let imported = context.import(&importer, &record, VALID_PNG).unwrap();
    assert_eq!(imported.image().pixels(), &[24, 120, 220, 255]);
    assert_eq!(
        importer.budget_snapshot().high_water_bytes(),
        imported.memory_plan().peak_bytes()
    );
    drop(imported);

    assert!(matches!(
        context
            .import(&importer, &record, ENCODED_PLUS_ONE_PNG)
            .unwrap_err(),
        ImageImportError::Budget {
            stage: ImageImportStage::Admission,
            error,
        }
            if error.kind() == ImageImportLimitKind::EncodedBytes
                && error.observed() == Some(ENCODED_PLUS_ONE_PNG.len() as u64)
    ));
    assert!(matches!(
        context
            .import(&importer, &record, INTERLACED_PNG)
            .unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Header,
            feature: ImageUnsupportedFeature::Interlacing,
        }
    ));
    let dimension_limited =
        ImageImporter::with_limits(limits.with_max_width(std::num::NonZeroU32::new(1).unwrap()))
            .unwrap();
    assert!(matches!(
        context
            .import(&dimension_limited, &record, WIDTH_PLUS_ONE_PNG)
            .unwrap_err(),
        ImageImportError::Budget { error, .. }
            if error.kind() == ImageImportLimitKind::Width && error.observed() == Some(2)
    ));
    for malformed in [INVALID_CRC_PNG, TRUNCATED_PNG, OVERSIZED_ANCILLARY_PNG] {
        assert!(matches!(
            context.import(&importer, &record, malformed).unwrap_err(),
            ImageImportError::Png { .. }
        ));
    }
    let metadata_importer = ImageImporter::default();
    assert!(matches!(
        context
            .import(&metadata_importer, &record, EXIF_PNG)
            .unwrap_err(),
        ImageImportError::Unsupported {
            stage: ImageImportStage::Metadata,
            feature: ImageUnsupportedFeature::EmbeddedMetadata,
        }
    ));
    assert_budget_released(importer.budget_snapshot());
    assert_budget_released(dimension_limited.budget_snapshot());
    assert_budget_released(metadata_importer.budget_snapshot());
}

#[test]
fn root_facade_plugin_reload_preserves_last_good_after_budget_rejection() {
    let root = TempRoot::new();
    let record = image_record();
    root.write(record.path().as_str(), VALID_PNG);
    let limits = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(VALID_PNG.len()).unwrap());
    let mut app = App::new();
    app.insert_resource(TaskPools::inline_for_tests(TaskPoolConfig::default()).unwrap())
        .unwrap();
    app.add_plugins((
        nara::reflect::ComponentRegistryPlugin,
        nara::tasks::TaskPlugin::default(),
        nara::asset::AssetPlugin,
        nara::hierarchy::HierarchyPlugin,
        nara::transform::TransformPlugin,
        nara::render::RenderPlugin,
        ImagePreparePlugin,
        ImagePlugin::with_limits(limits)
            .unwrap()
            .with_source_directory(source_directory(root.path())),
    ))
    .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ProjectAssetDatabase>()
        .insert(record.clone())
        .unwrap();
    let handle = app
        .world_mut()
        .unwrap()
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&record)
        .unwrap();

    request_reload(&mut app, &record);
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

    root.write(record.path().as_str(), ENCODED_PLUS_ONE_PNG);
    request_reload(&mut app, &record);
    drive_image_jobs(&mut app);

    let image = app
        .world()
        .resource::<Assets<ImageAsset>>()
        .get(handle)
        .unwrap();
    assert_eq!(image.source().source_hash(), last_good_hash);
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
    let diagnostic = app
        .world()
        .resource::<AssetReloadDiagnostics>()
        .iter()
        .next()
        .unwrap();
    assert_eq!(diagnostic.code().as_str(), "image.import-budget-exceeded");
    assert_eq!(
        diagnostic_field(diagnostic, "asset-path"),
        "textures/player.png"
    );
    assert_eq!(diagnostic_field(diagnostic, "stage"), "source-read");
    assert_eq!(diagnostic_field(diagnostic, "limit-kind"), "encoded-bytes");
    assert_eq!(
        diagnostic_field(diagnostic, "limit"),
        VALID_PNG.len().to_string()
    );
    assert_eq!(
        diagnostic_field(diagnostic, "observed"),
        ENCODED_PLUS_ONE_PNG.len().to_string()
    );
    assert_budget_released(app.world().resource::<ImageImporter>().budget_snapshot());
}

fn image_record() -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(TEXTURE_ID).unwrap(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    )
}

struct ImagePublicationContext {
    server: AssetServer,
    handle: Handle<ImageAsset>,
    images: Assets<ImageAsset>,
    states: AssetStates,
}

impl ImagePublicationContext {
    fn new(record: &AssetRecord) -> Self {
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(record).unwrap();
        Self {
            server,
            handle,
            images: Assets::default(),
            states: AssetStates::default(),
        }
    }

    fn import(
        &self,
        importer: &ImageImporter,
        record: &AssetRecord,
        bytes: &[u8],
    ) -> Result<ImageImportedAsset, ImageImportError> {
        importer.import_image(
            import_request(record, bytes),
            self.handle,
            AssetVersion::ZERO,
            &self.server,
            &self.images,
            &self.states,
        )
    }
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

fn request_reload(app: &mut App, record: &AssetRecord) {
    app.world_mut()
        .unwrap()
        .resource_mut::<AssetSourceChanges>()
        .modified(record.path().clone())
        .unwrap();
}

fn drive_image_jobs(app: &mut App) {
    app.update().unwrap();
    let _ = app.world().resource::<TaskPools>().run_pending_for_tests();
    app.update().unwrap();
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

        return fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap();
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
            "nara_root_image_limits_{}_{}",
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
