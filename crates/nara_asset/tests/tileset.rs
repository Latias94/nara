use nara_asset::{
    image::{AddressMode, FilterMode, SamplerDescriptor},
    tilemap::{
        MAX_TILESET_TILES, TileAtlasLayout, TileDefinition, TileSetAsset, TileSetAssetError,
    },
};
use nara_core::{AssetId, Vec2};

const IMAGE_ID: &str = "0193a100-1111-7000-8000-000000000001";
#[cfg(any(feature = "cooked", feature = "runtime"))]
const TILESET_ID: &str = "0193a100-1111-7000-8000-000000000002";

#[cfg(feature = "authoring")]
const SOURCE: &str = r#"TileSet(
    image: "0193a100-1111-7000-8000-000000000001",
    layout: (tile_size: (1.0, 1.0), columns: 1, rows: 1),
    tiles: [(solid: true)],
)"#;

fn fixture() -> TileSetAsset {
    TileSetAsset {
        image: IMAGE_ID.parse::<AssetId>().unwrap(),
        layout: TileAtlasLayout::grid(Vec2::new(16.0, 8.0), 3, 2)
            .with_margin(Vec2::new(2.0, 1.0))
            .with_spacing(Vec2::new(1.0, 2.0)),
        sampler: SamplerDescriptor {
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            address_mode_u: AddressMode::Repeat,
            address_mode_v: AddressMode::MirrorRepeat,
        },
        tiles: vec![
            TileDefinition {
                solid: true,
                ..TileDefinition::default()
            },
            TileDefinition {
                one_way: true,
                collision_group: 1 << 31,
                ..TileDefinition::default()
            },
            TileDefinition::default(),
        ],
    }
}

#[test]
fn cooked_tileset_matches_the_wire_and_reencodes_identically() {
    let asset = fixture();
    let bytes = asset.encode_cooked().unwrap();
    assert_eq!(&bytes[..12], b"NTLS\x01\0\0\x01\0\x01\x02\0");
    assert_eq!(&bytes[12..28], asset.image.as_uuid().as_bytes());
    assert_eq!(&bytes[60..64], &3u32.to_le_bytes());
    assert_eq!(
        &bytes[64..],
        &[1, 0, 0, 0, 0, 2, 0, 0, 0, 0x80, 0, 0, 0, 0, 0]
    );
    let decoded = TileSetAsset::from_cooked(&bytes).unwrap();
    assert_eq!(decoded, asset);
    assert_eq!(decoded.encode_cooked().unwrap(), bytes);
}

#[test]
fn cooked_tileset_rejects_short_trailing_and_malformed_data() {
    let bytes = fixture().encode_cooked().unwrap();
    for len in 0..bytes.len() {
        assert!(TileSetAsset::from_cooked(&bytes[..len]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(TileSetAsset::from_cooked(&trailing).is_err());

    let mutations = [
        (0, vec![0]),
        (6, vec![2]),
        (10, vec![3]),
        (11, vec![1]),
        (12, vec![0; 16]),
        (28, 0u32.to_le_bytes().to_vec()),
        (36, f32::NAN.to_le_bytes().to_vec()),
        (44, (-1.0f32).to_le_bytes().to_vec()),
        (52, f32::INFINITY.to_le_bytes().to_vec()),
        (60, 7u32.to_le_bytes().to_vec()),
        (64, vec![4]),
    ];
    for (offset, replacement) in mutations {
        let mut broken = bytes.clone();
        broken[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            TileSetAsset::from_cooked(&broken).is_err(),
            "field at {offset}"
        );
    }

    let mut wrong_version = bytes;
    wrong_version[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert!(matches!(
        TileSetAsset::from_cooked(&wrong_version),
        Err(TileSetAssetError::UnsupportedVersion(2)),
    ));
}

#[test]
fn tile_count_matches_the_packed_u16_cell_limit() {
    let mut asset = fixture();
    let too_many = MAX_TILESET_TILES + 1;
    asset.layout = TileAtlasLayout::grid(Vec2::ONE, too_many as u32, 1);
    asset
        .tiles
        .resize(MAX_TILESET_TILES, TileDefinition::default());
    let mut bytes = asset.encode_cooked().unwrap();
    assert_eq!(TileSetAsset::from_cooked(&bytes).unwrap(), asset);

    bytes[60..64].copy_from_slice(&(too_many as u32).to_le_bytes());
    assert!(matches!(
        TileSetAsset::from_cooked(&bytes),
        Err(TileSetAssetError::TooManyTiles { count }) if count == too_many,
    ));
    asset.tiles.push(TileDefinition::default());
    assert!(matches!(
        asset.encode_cooked(),
        Err(TileSetAssetError::TooManyTiles { count }) if count == too_many,
    ));
}

#[cfg(feature = "authoring")]
#[test]
fn source_reads_a_minimal_tileset_and_cooks_its_defaults() {
    let asset = TileSetAsset::from_ron(SOURCE).unwrap();
    assert_eq!(asset.image, IMAGE_ID.parse::<AssetId>().unwrap());
    assert_eq!(asset.layout, TileAtlasLayout::grid(Vec2::ONE, 1, 1));
    assert_eq!(asset.sampler, SamplerDescriptor::LINEAR_CLAMP);
    assert_eq!(
        asset.tiles,
        vec![TileDefinition {
            solid: true,
            one_way: false,
            collision_group: 0,
        }],
    );
    let bytes = asset.encode_cooked().unwrap();
    assert_eq!(TileSetAsset::from_cooked(&bytes).unwrap(), asset);
}

#[cfg(feature = "authoring")]
#[test]
fn source_rejects_bad_layout_image_and_excess_definitions() {
    use nara_asset::tilemap::MAX_TILESET_SOURCE_BYTES;

    for (field, replacement) in [
        (IMAGE_ID, "00000000-0000-0000-0000-000000000000"),
        ("columns: 1", "columns: 0"),
        ("1.0, 1.0", "0.0, 1.0"),
        ("solid: true", "solid: 1"),
        ("(solid: true)", "(), ()"),
    ] {
        assert!(TileSetAsset::from_ron(&SOURCE.replace(field, replacement)).is_err());
    }
    let oversized = " ".repeat(MAX_TILESET_SOURCE_BYTES + 1);
    assert!(matches!(
        TileSetAsset::from_ron(&oversized),
        Err(TileSetAssetError::SourceTooLarge),
    ));
    let too_many = SOURCE
        .replace("columns: 1", "columns: 65536")
        .replace("(solid: true)", &"(),".repeat(MAX_TILESET_TILES + 1));
    assert!(matches!(
        TileSetAsset::from_ron(&too_many),
        Err(TileSetAssetError::Ron(error)) if error.to_string().contains("65535-tile limit"),
    ));
}

#[cfg(feature = "cooked")]
#[test]
fn npak_v1_carries_tileset_tag_eight_alongside_existing_kinds() {
    use nara_asset::cooked::{NpakArchive, NpakCompression, NpakEntryKind, NpakInput, encode_npak};

    let asset = fixture();
    let id: AssetId = TILESET_ID.parse().unwrap();
    let cooked = asset.encode_cooked().unwrap();
    let bytes = encode_npak(&[
        NpakInput::new(id, NpakEntryKind::TileSet, &cooked, NpakCompression::Lz4),
        NpakInput::new(
            asset.image,
            NpakEntryKind::Image,
            b"image",
            NpakCompression::None,
        ),
    ])
    .unwrap();
    assert_eq!(&bytes[..6], b"NPAK\x01\0");
    assert_eq!(NpakEntryKind::TileSet as u64, 8);
    let archive = NpakArchive::from_bytes(bytes).unwrap();
    let payload = archive.read_kind(id, NpakEntryKind::TileSet).unwrap();
    assert_eq!(TileSetAsset::from_cooked(&payload).unwrap(), asset);
    assert_eq!(
        archive
            .read_kind(asset.image, NpakEntryKind::Image)
            .unwrap(),
        b"image"
    );
}

#[cfg(feature = "runtime")]
#[test]
fn runtime_loader_checks_image_metadata_without_fetching_image_bytes() {
    use nara_asset::{
        AssetError, AssetLoadError, NpakEntryKind, RuntimeAssetMode, RuntimeAssetSource,
        prepare_asset,
        source::asset_id_path,
        tilemap::{TileSetLoader, TileSetLoaderError},
    };
    use nara_core::vfs::MemoryAssetSource;

    let asset = fixture();
    let id: AssetId = TILESET_ID.parse().unwrap();
    let cooked = asset.encode_cooked().unwrap();
    let source = |image_kind| {
        let mut files = MemoryAssetSource::new();
        files.insert(&asset_id_path(id), cooked.clone()).unwrap();
        RuntimeAssetSource::from_memory_with_kinds(
            RuntimeAssetMode::Cooked,
            files,
            [(id, NpakEntryKind::TileSet), (asset.image, image_kind)],
        )
    };
    let prepared = prepare_asset(&source(NpakEntryKind::Image), id, &TileSetLoader).unwrap();
    assert_eq!(prepared.value(), &asset);
    assert!(prepared.dependencies().is_empty());

    let result = prepare_asset(&source(NpakEntryKind::Raw), id, &TileSetLoader);
    let Err(AssetLoadError::Decode { source, .. }) = result else {
        panic!("expected a tile set loader error");
    };
    assert!(matches!(
        source.downcast_ref::<TileSetLoaderError>(),
        Some(TileSetLoaderError::ImageKind(AssetError::WrongRuntimeKind {
            id,
            expected: NpakEntryKind::Image,
            actual: NpakEntryKind::Raw,
        })) if *id == asset.image,
    ));
}

#[cfg(all(feature = "runtime", feature = "authoring", any(unix, windows)))]
#[test]
fn project_discovery_loads_a_tileset_by_its_generated_kind() {
    use std::{fs, io};

    use nara_asset::{
        AssetLoadError, AssetScanMode, AssetSourcePath, NpakEntryKind, ProjectAssetDatabase,
        RuntimeAssetSource, prepare_asset,
        tilemap::{MAX_TILESET_SOURCE_BYTES, TileSetLoader},
    };

    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("assets");
    fs::create_dir(&directory).unwrap();
    let tile_path = directory.join("world.TILESET.RON");
    fs::write(&tile_path, SOURCE).unwrap();
    fs::write(directory.join("atlas.png"), []).unwrap();
    fs::write(
        directory.join("atlas.png.meta"),
        format!(r#"(id: "{IMAGE_ID}", importer: "nara.image", settings: ())"#),
    )
    .unwrap();
    let scope = AssetSourcePath::new("assets").unwrap();
    let path = AssetSourcePath::new("assets/world.TILESET.RON").unwrap();
    let database =
        ProjectAssetDatabase::scan(root.path(), &[scope], AssetScanMode::Discover).unwrap();
    let record = database.record_for_path(&path).unwrap();
    assert_eq!(record.meta().unwrap().importer(), "nara.tileset");
    let id = AssetId::try_from(record.stable_id()).unwrap();
    let source = RuntimeAssetSource::from_project_dir(root.path(), "assets").unwrap();
    assert_eq!(source.backend().kind(id), Some(NpakEntryKind::TileSet));
    let prepared = prepare_asset(&source, id, &TileSetLoader).unwrap();
    assert_eq!(prepared.value(), &TileSetAsset::from_ron(SOURCE).unwrap());

    fs::File::create(tile_path)
        .unwrap()
        .set_len((MAX_TILESET_SOURCE_BYTES + 1) as u64)
        .unwrap();
    assert!(matches!(
        prepare_asset(&source, id, &TileSetLoader),
        Err(AssetLoadError::Read { source, .. }) if source.kind() == io::ErrorKind::FileTooLarge,
    ));
}
