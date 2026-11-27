#![cfg(feature = "cooked")]

use std::{io, sync::Arc};

use nara_asset::{
    cooked::{
        MAX_NPAK_ENTRY_BYTES, NpakArchive, NpakCompression, NpakEntryKind, NpakError, NpakInput,
        encode_npak,
    },
    image::{CookedImageError, ImageAsset, ImageColorSpace, ImageExtent, ImageFormat},
    source::{asset_id_path, read_asset},
};
use nara_core::{
    AssetId,
    vfs::{AssetPath, AssetSource, MemoryAssetSource},
};

fn id(value: u128) -> AssetId {
    AssetId::try_from_uuid(uuid::Uuid::from_u128(value)).unwrap()
}

#[test]
fn archive_matches_the_fixed_wire_and_sorts_payloads_by_uuid_bytes() {
    let first = NpakInput::new(id(9), NpakEntryKind::Scene, b"abc", NpakCompression::None);
    let second = NpakInput::new(id(256), NpakEntryKind::Shader, b"xy", NpakCompression::Lz4);
    let bytes = encode_npak(&[second, first]).unwrap();
    assert_eq!(bytes, encode_npak(&[first, second]).unwrap());
    assert_eq!(
        &bytes[..31],
        b"NPAK\x01\x00\x02\x00\x00\x00\x1f\x00\x00\x00\x00\x00\x00\x00\
          \x68\x00\x00\x00\x00\x00\x00\x00abcxy"
    );
    let first_row = &bytes[31..83];
    assert_eq!(&first_row[..16], &9u128.to_be_bytes());
    assert_eq!(&first_row[16..24], &1u64.to_le_bytes());
    assert_eq!(&first_row[24..32], &26u64.to_le_bytes());
    assert_eq!(&first_row[32..40], &3u64.to_le_bytes());
    assert_eq!(&first_row[40..48], &3u64.to_le_bytes());
    assert_eq!(&first_row[48..52], &0u32.to_le_bytes());

    let archive = NpakArchive::from_bytes(bytes).unwrap();
    assert_eq!(archive.entries()[1].compression, NpakCompression::None);
    assert_eq!(
        archive.read_kind(id(9), NpakEntryKind::Scene).unwrap(),
        b"abc"
    );
    assert!(matches!(
        archive.read_kind(id(9), NpakEntryKind::Material),
        Err(NpakError::WrongKind { .. })
    ));
    assert!(matches!(
        encode_npak(&[first, second, first]),
        Err(NpakError::DuplicateId(duplicate)) if duplicate == id(9)
    ));
}

#[test]
fn archive_rejects_truncated_unsorted_and_unbounded_directories() {
    let bytes = encode_npak(&[
        NpakInput::new(id(1), NpakEntryKind::Raw, b"1", NpakCompression::None),
        NpakInput::new(id(2), NpakEntryKind::Raw, b"2", NpakCompression::None),
    ])
    .unwrap();
    let toc = 28;
    let mutations = [
        (4, 2u16.to_le_bytes().to_vec()),
        (6, u32::MAX.to_le_bytes().to_vec()),
        (10, u64::MAX.to_le_bytes().to_vec()),
        (18, 105u64.to_le_bytes().to_vec()),
        (toc, [0; 16].to_vec()),
        (toc + 16, 99u64.to_le_bytes().to_vec()),
        (toc + 24, 0u64.to_le_bytes().to_vec()),
        (toc + 32, 2u64.to_le_bytes().to_vec()),
        (
            toc + 40,
            (MAX_NPAK_ENTRY_BYTES as u64 + 1).to_le_bytes().to_vec(),
        ),
        (toc + 48, 2u32.to_le_bytes().to_vec()),
        (toc + 52, 1u128.to_be_bytes().to_vec()),
    ];
    for (offset, replacement) in mutations {
        let mut broken = bytes.clone();
        broken[offset..offset + replacement.len()].copy_from_slice(&replacement);
        assert!(
            NpakArchive::from_bytes(broken).is_err(),
            "accepted invalid field at byte {offset}"
        );
    }
    assert!(NpakArchive::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    let mut trailing = bytes;
    trailing.push(0);
    assert!(NpakArchive::from_bytes(trailing).is_err());
}

#[test]
fn compressed_entries_read_independently_and_share_the_memory_source_namespace() {
    let data = vec![17u8; 4096];
    let mut bytes = encode_npak(&[
        NpakInput::new(id(7), NpakEntryKind::Raw, &data, NpakCompression::Lz4),
        NpakInput::new(id(8), NpakEntryKind::Raw, b"safe", NpakCompression::None),
        NpakInput::new(id(9), NpakEntryKind::Raw, b"", NpakCompression::Lz4),
    ])
    .unwrap();
    let shared: Arc<[u8]> = Arc::from(bytes.as_slice());
    let archive = NpakArchive::from_bytes(shared).unwrap();
    let compressed = archive.entry(id(7)).unwrap();
    assert_eq!(compressed.compression, NpakCompression::Lz4);
    assert!(compressed.stored_len < compressed.uncompressed_len);
    assert_eq!(archive.read(id(7)).unwrap(), data);
    assert!(archive.read(id(9)).unwrap().is_empty());

    let mut memory = MemoryAssetSource::new();
    memory.insert(&asset_id_path(id(8)), b"safe").unwrap();
    for source in [&archive as &dyn AssetSource, &memory] {
        assert_eq!(read_asset(source, id(8)).unwrap(), b"safe");
        assert_eq!(
            read_asset(source, id(1)).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        let child = AssetPath::new(format!("{}/child", asset_id_path(id(8)))).unwrap();
        assert_eq!(
            source.read_bytes(&child).unwrap_err().kind(),
            io::ErrorKind::NotADirectory
        );
    }
    assert_eq!(
        archive.list_dir(&AssetPath::ROOT).unwrap(),
        [
            asset_id_path(id(7)),
            asset_id_path(id(8)),
            asset_id_path(id(9))
        ]
    );
    assert!(!archive.exists(&AssetPath::new(id(8).as_uuid().simple().to_string()).unwrap()));

    let start = compressed.offset as usize;
    bytes[start..start + compressed.stored_len as usize].fill(255);
    let broken = NpakArchive::from_bytes(bytes).unwrap();
    assert!(matches!(
        broken.read(id(7)),
        Err(NpakError::Decompress { .. })
    ));
    assert_eq!(read_asset(&broken, id(8)).unwrap(), b"safe");
}

#[cfg(any(unix, windows))]
#[test]
fn file_archive_reads_payloads_lazily_from_the_open_file() {
    use nara_asset::cooked::write_npak;

    let mut file = tempfile::NamedTempFile::new().unwrap();
    write_npak(
        file.as_file_mut(),
        &[
            NpakInput::new(id(2), NpakEntryKind::Raw, b"keep", NpakCompression::None),
            NpakInput::new(id(3), NpakEntryKind::Raw, b"lost", NpakCompression::None),
        ],
    )
    .unwrap();
    let archive = NpakArchive::open(file.path()).unwrap();
    file.as_file()
        .set_len(archive.entry(id(3)).unwrap().offset)
        .unwrap();
    assert_eq!(archive.read(id(2)).unwrap(), b"keep");
    assert!(matches!(
        archive.read(id(3)),
        Err(NpakError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof
    ));
}

#[test]
fn image_payload_needs_no_png_decoder_and_checks_its_extent_and_exact_length() {
    let image = ImageAsset::new(
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        ImageColorSpace::Linear,
        vec![11, 22, 33, 44],
    )
    .unwrap();
    let bytes = image.encode_cooked().unwrap();
    assert_eq!(
        &bytes[..16],
        b"NIMG\x01\x00\x01\x02\x01\x00\x00\x00\x01\x00\x00\x00"
    );
    assert_eq!(ImageAsset::from_cooked(&bytes).unwrap(), image);
    assert!(ImageAsset::from_cooked(&bytes[..19]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(ImageAsset::from_cooked(&trailing).is_err());
    let mut oversized = bytes;
    oversized[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        ImageAsset::from_cooked(&oversized),
        Err(CookedImageError::ExtentLimit { .. })
    ));
}
