use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use image::ImageEncoder;
use nara_asset::{
    AssetEvents, AssetProductRef, AssetRecord, AssetServer, AssetSlotRevision, AssetSourceKind,
    AssetSourcePath, AssetStateRevision, AssetStates, AssetVersion, Assets, Handle,
    ImportedProductId, StableAssetId,
};
use nara_fs::{FileCapability, TrustMode};
use nara_tasks::TaskCancellationToken;

use crate::{
    ImageAsset, ImageBytesImportRequest, ImageColorSpace, ImageExtent, ImageFileImportRequest,
    ImageFormat, ImageImportBudgetSnapshot, ImageImportError, ImageImportSettings, ImageImporter,
    ImagePublicationFailureKind,
};

const PIXELS: [u8; 4] = [24, 120, 220, 255];

fn record(id: &str, path: &str) -> AssetRecord {
    AssetRecord::new(
        StableAssetId::parse_str(id).unwrap(),
        AssetSourcePath::new(path).unwrap(),
        AssetSourceKind::Image,
    )
}

fn requested_source() -> AssetRecord {
    record(
        "20000000-0000-4000-8000-000000000001",
        "textures/source.png",
    )
}

fn rejected_sources() -> [Option<AssetRecord>; 3] {
    [
        None,
        Some(record(
            "20000000-0000-4000-8000-000000000001",
            "textures/other.png",
        )),
        Some(record(
            "20000000-0000-4000-8000-000000000002",
            "textures/source.png",
        )),
    ]
}

fn png_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&PIXELS, 1, 1, image::ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

struct Fixture {
    source: AssetRecord,
    handle: Handle<ImageAsset>,
    server: AssetServer,
    images: Assets<ImageAsset>,
    states: AssetStates,
    events: AssetEvents,
    version: AssetVersion,
}

impl Fixture {
    fn new(previous_source: Option<AssetRecord>) -> Self {
        let source = requested_source();
        let image = match previous_source {
            Some(previous) => ImageImporter::default()
                .preflight_unpublished_import(ImageBytesImportRequest::new(
                    previous,
                    png_bytes(),
                    ImageImportSettings::new(ImageColorSpace::Linear),
                ))
                .unwrap()
                .import()
                .unwrap(),
            None => ImageAsset::from_product(
                AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY).unwrap(),
                ImageExtent::new(1, 1),
                ImageFormat::Rgba8,
                ImageColorSpace::Linear,
                PIXELS.to_vec(),
            )
            .unwrap(),
        };
        let mut server = AssetServer::new();
        let handle = server.reserve_record::<ImageAsset>(&source).unwrap();
        let mut images = Assets::default();
        let mut states = AssetStates::default();
        let mut events = AssetEvents::default();
        let version = images
            .commit_loaded(handle, image, &mut states, &mut events, None, None)
            .unwrap();
        Self {
            source,
            handle,
            server,
            images,
            states,
            events,
            version,
        }
    }

    fn baseline(&self) -> (AssetSlotRevision, AssetStateRevision, usize, *const u8) {
        (
            self.images.slot_revision(self.handle),
            self.states.state_revision(self.handle.id()),
            self.events.len(),
            self.images.get(self.handle).unwrap().pixels().as_ptr(),
        )
    }
}

struct PngFile(PathBuf);

impl PngFile {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nara-image-source-admission-{}-{stamp}.png",
            std::process::id()
        ));
        let mut file = fs::File::create_new(&path).unwrap();
        std::io::Write::write_all(&mut file, &png_bytes()).unwrap();
        Self(path)
    }

    fn open(&self) -> FileCapability {
        FileCapability::from_host_handle(
            fs::File::open(&self.0).unwrap(),
            TrustMode::TrustedLocal,
            0,
        )
        .unwrap()
    }
}

impl Drop for PngFile {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}

#[test]
fn bytes_import_refuses_existing_images_without_matching_source_before_decode() {
    for previous in rejected_sources() {
        let fixture = Fixture::new(previous);
        let importer = ImageImporter::default();
        let baseline = fixture.baseline();
        let result = importer.import_image(
            ImageBytesImportRequest::new(
                fixture.source.clone(),
                png_bytes(),
                ImageImportSettings::default(),
            ),
            fixture.handle,
            fixture.version,
            &fixture.server,
            &fixture.images,
            &fixture.states,
        );
        assert_eq!(
            result.unwrap_err(),
            ImageImportError::Publication(ImagePublicationFailureKind::TargetMismatch,)
        );
        assert_eq!(
            importer.budget_snapshot(),
            ImageImportBudgetSnapshot::default()
        );
        assert_eq!(fixture.baseline(), baseline);
        assert_eq!(
            fixture.states.version(fixture.handle.id()),
            Some(fixture.version)
        );
    }
}

#[test]
fn file_import_refuses_existing_images_without_matching_source_before_read_admission() {
    let file = PngFile::new();
    for previous in rejected_sources() {
        let fixture = Fixture::new(previous);
        let importer = ImageImporter::default();
        let baseline = fixture.baseline();
        let result = importer.admit_file(
            ImageFileImportRequest::new(
                fixture.source.clone(),
                file.open(),
                ImageImportSettings::default(),
            ),
            fixture.handle,
            fixture.version,
            &fixture.server,
            &fixture.images,
            &fixture.states,
        );
        assert_eq!(
            result.unwrap_err(),
            ImageImportError::Publication(ImagePublicationFailureKind::TargetMismatch,)
        );
        assert_eq!(
            importer.budget_snapshot(),
            ImageImportBudgetSnapshot::default()
        );
        assert_eq!(fixture.baseline(), baseline);
        assert_eq!(
            fixture.states.version(fixture.handle.id()),
            Some(fixture.version)
        );
    }
}

#[test]
fn matching_source_allows_bytes_and_file_reimport_and_public_commit() {
    let file = PngFile::new();
    for from_file in [false, true] {
        let mut fixture = Fixture::new(Some(requested_source()));
        let importer = ImageImporter::default();
        let baseline = fixture.baseline();
        let settings = ImageImportSettings::new(ImageColorSpace::Srgb);
        let imported = if from_file {
            importer
                .admit_file(
                    ImageFileImportRequest::new(fixture.source.clone(), file.open(), settings),
                    fixture.handle,
                    fixture.version,
                    &fixture.server,
                    &fixture.images,
                    &fixture.states,
                )
                .unwrap()
                .import(TaskCancellationToken::new())
                .unwrap()
        } else {
            importer
                .import_image(
                    ImageBytesImportRequest::new(fixture.source.clone(), png_bytes(), settings),
                    fixture.handle,
                    fixture.version,
                    &fixture.server,
                    &fixture.images,
                    &fixture.states,
                )
                .unwrap()
        };
        assert_eq!(fixture.baseline(), baseline);
        assert_eq!(imported.source().settings(), settings);
        let version = imported
            .commit(
                &fixture.server,
                &mut fixture.images,
                &mut fixture.states,
                &mut fixture.events,
            )
            .unwrap();
        assert_ne!(version, fixture.version);
        let image = fixture.images.get(fixture.handle).unwrap();
        assert_eq!(image.source().unwrap().path(), fixture.source.path());
        assert_eq!(
            image.source().unwrap().stable_id(),
            fixture.source.stable_id()
        );
        assert_eq!(image.color_space(), ImageColorSpace::Srgb);
        assert_eq!(image.pixels(), &PIXELS);
        assert_eq!(importer.budget_snapshot().active_bytes(), 0);
    }
}
