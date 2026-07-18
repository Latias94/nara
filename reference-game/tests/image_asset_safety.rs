use std::{fs::File, path::Path};

use nara::advanced_prelude::*;

const TEXTURE_ID: &str = "2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f";
const TEXTURE_BYTES: &[u8] = include_bytes!("../assets/textures/player.png");

#[test]
fn independent_game_imports_committed_png_through_the_public_bounded_path() {
    let record = AssetRecord::new(
        StableAssetId::parse_str(TEXTURE_ID).unwrap(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    );
    let source = source_directory(&Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"));
    let limits = ImageImportLimits::default()
        .with_max_encoded_bytes(ByteLimit::new(TEXTURE_BYTES.len()).unwrap());
    let importer = ImageImporter::with_limits(limits).unwrap();
    let mut server = AssetServer::new();
    let handle = server.reserve_record::<ImageAsset>(&record).unwrap();
    let mut images = Assets::<ImageAsset>::default();
    let mut states = AssetStates::default();
    let mut events = AssetEvents::default();
    let imported = import_file(
        &importer,
        &source,
        record.clone(),
        handle,
        AssetVersion::ZERO,
        &server,
        &images,
        &states,
    )
    .unwrap();
    assert_eq!(imported.image().pixels(), &[24, 120, 220, 255]);
    imported
        .commit(&server, &mut images, &mut states, &mut events)
        .unwrap();
    let last_good_hash = images.get(handle).unwrap().source().source_hash();
    let last_good_version = states.version(handle.id()).unwrap();
    assert_budget_released(importer.budget_snapshot());

    let limited = ImageImporter::with_limits(
        limits.with_max_encoded_bytes(ByteLimit::new(TEXTURE_BYTES.len() - 1).unwrap()),
    )
    .unwrap();
    assert!(matches!(
        import_file(
            &limited,
            &source,
            record,
            handle,
            last_good_version,
            &server,
            &images,
            &states,
        )
        .unwrap_err(),
        ImageImportError::Budget { error, .. }
            if error.kind() == ImageImportLimitKind::EncodedBytes
                && error.observed() == Some(TEXTURE_BYTES.len() as u64)
    ));
    assert_eq!(
        images.get(handle).unwrap().source().source_hash(),
        last_good_hash
    );
    assert_eq!(states.version(handle.id()).unwrap(), last_good_version);
    assert_budget_released(limited.budget_snapshot());
}

fn import_file(
    importer: &ImageImporter,
    source: &ImageSourceDirectory,
    record: AssetRecord,
    handle: Handle<ImageAsset>,
    expected_version: AssetVersion,
    server: &AssetServer,
    images: &Assets<ImageAsset>,
    states: &AssetStates,
) -> Result<ImageImportedAsset, ImageImportError> {
    let file = source.open(&record)?;
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
            expected_version,
            server,
            images,
            states,
        )?
        .import(TaskCancellationToken::new())
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

        return std::fs::OpenOptions::new()
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

fn assert_budget_released(snapshot: ImageImportBudgetSnapshot) {
    assert_eq!(snapshot.active_reservations(), 0);
    assert_eq!(snapshot.active_bytes(), 0);
    assert_eq!(
        snapshot.total_reserved_bytes(),
        snapshot.total_released_bytes()
    );
    assert_eq!(
        snapshot.accepted_reservations(),
        snapshot.released_reservations()
    );
}
