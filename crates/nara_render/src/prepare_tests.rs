use nara_asset::{AssetVersion, Assets, allocate_anonymous_handle};

use crate::{
    RenderResourceContentHash, RenderResourceKey, RenderResourceKind, RenderResourceSnapshot,
};

#[test]
fn prepared_content_hash_matches_blake3_and_accepts_precomputed_digest() {
    const DIGEST: [u8; 32] = [
        0x64, 0x37, 0xb3, 0xac, 0x38, 0x46, 0x51, 0x33, 0xff, 0xb6, 0x3b, 0x75, 0x27, 0x3a, 0x8d,
        0xb5, 0x48, 0xc5, 0x58, 0x46, 0x5d, 0x79, 0xdb, 0x03, 0xfd, 0x35, 0x9c, 0x6c, 0xd5, 0xbd,
        0x9d, 0x85,
    ];
    const PRECOMPUTED: RenderResourceContentHash = RenderResourceContentHash::from_digest(DIGEST);

    let content_hash = RenderResourceContentHash::from_bytes(b"abc");
    assert_eq!(content_hash, PRECOMPUTED);
    assert_eq!(content_hash.as_bytes(), DIGEST);
    assert_ne!(RenderResourceContentHash::from_bytes(b"abd"), content_hash);
}

#[test]
fn same_content_replacement_keeps_distinct_slot_identity() {
    let handle = allocate_anonymous_handle::<Vec<u8>>().unwrap();
    let mut assets = Assets::default();
    let content = b"prepared descriptor and data".to_vec();
    let content_hash = RenderResourceContentHash::from_bytes(&content);
    let key = RenderResourceKey::for_asset(handle, RenderResourceKind::new("test.prepared"));
    let version = AssetVersion::from_raw(7);
    assets.insert(handle, content.clone());
    let first =
        RenderResourceSnapshot::new(key, version, assets.slot_revision(handle), content_hash);

    assets.insert(handle, content);
    let replacement =
        RenderResourceSnapshot::new(key, version, assets.slot_revision(handle), content_hash);

    assert_eq!(first.content_hash(), replacement.content_hash());
    assert_ne!(first.slot_revision(), replacement.slot_revision());
    assert_ne!(first, replacement);
}
