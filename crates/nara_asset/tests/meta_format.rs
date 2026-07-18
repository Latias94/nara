#![cfg(feature = "serde")]

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaFileLimits, AssetPath, AssetSourceKind, StableAssetId,
};
use nara_core::{ByteLimit, SerdeShapeLimits};

#[test]
fn asset_meta_round_trips_through_the_canonical_envelope() {
    let meta = sample_meta();
    let encoded = meta.to_json_string().unwrap();

    let candidate = AssetMetaCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();

    assert_eq!(candidate.into_meta(), meta);
    assert!(encoded.contains("\"kind\": \"asset_meta\""));
    assert!(encoded.contains("\"format_version\": 1"));
}

#[test]
fn asset_meta_rejects_unknown_fields_and_the_encoded_sentinel_byte() {
    let encoded = sample_meta().to_json_string().unwrap();
    let mut value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    value["payload"]["unexpected"] = serde_json::Value::Bool(true);
    let hostile = serde_json::to_vec(&value).unwrap();
    assert!(AssetMetaCandidate::decode_json_bytes(&hostile).is_err());

    let exact =
        AssetMetaFileLimits::default().with_encoded_bytes(ByteLimit::new(encoded.len()).unwrap());
    assert!(AssetMetaCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), exact).is_ok());
    let short = AssetMetaFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(encoded.len() - 1).unwrap());
    assert!(AssetMetaCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), short).is_err());
}

#[test]
fn asset_meta_writer_enforces_the_same_shape_limits_as_the_decoder() {
    let mut meta = sample_meta();
    meta.source_kind = AssetSourceKind::Other("x".repeat(8 * 1024 + 1));

    assert!(matches!(
        meta.to_json_string(),
        Err(nara_asset::AssetMetaFormatError::Shape(_))
    ));

    let default = AssetMetaFileLimits::default();
    let shape = default.shape();
    let widened = default.with_shape(SerdeShapeLimits::new(
        shape.depth(),
        shape.nodes(),
        shape.container_items(),
        ByteLimit::new(16 * 1024).unwrap(),
        ByteLimit::new(48 * 1024).unwrap(),
    ));
    let encoded = meta.to_json_string_with_limits(widened).unwrap();
    let decoded = AssetMetaCandidate::decode_json_bytes_with_limits(encoded.as_bytes(), widened)
        .unwrap()
        .into_meta();

    assert_eq!(decoded, meta);
}

fn sample_meta() -> AssetMeta {
    AssetMeta::new(
        StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
        AssetPath::new("textures/player.png").unwrap(),
        AssetSourceKind::Image,
    )
}
