#[cfg(feature = "serde")]
use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaIdentity, AssetPath, AssetSourceKind, StableAssetId,
};
use nara_image::{ImageColorSpace, ImageImportSettings};

#[test]
fn image_settings_hash_is_deterministic_and_color_space_sensitive() {
    let srgb = ImageImportSettings::new(ImageColorSpace::Srgb);
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);

    assert_eq!(srgb.settings_hash(), srgb.settings_hash());
    assert_ne!(srgb.canonical_bytes(), linear.canonical_bytes());
    assert_ne!(srgb.settings_hash(), linear.settings_hash());
}

#[cfg(feature = "serde")]
#[test]
fn canonical_image_metadata_round_trips_both_recipes() {
    for color_space in [ImageColorSpace::Srgb, ImageColorSpace::Linear] {
        let meta = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
                AssetPath::new("textures/player.png").unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::new(color_space),
        );
        let encoded = meta.to_json_string().unwrap();
        let decoded =
            AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(encoded.as_bytes())
                .unwrap()
                .into_meta();

        assert_eq!(decoded, meta);
    }
}
