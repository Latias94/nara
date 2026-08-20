#![cfg(feature = "serde")]

use nara_asset::{
    AssetMeta, AssetMetaCandidate, AssetMetaFileLimits, AssetMetaIdentity,
    AssetMetaIdentityCandidate, AssetPath, AssetSourceKind, StableAssetId,
};
use nara_core::{ByteLimit, SerdeShapeLimits};
use uuid::{Variant, Version};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TestSettings {
    linear: bool,
}

#[test]
fn asset_meta_round_trips_through_the_canonical_nested_envelope() {
    let meta = sample_meta();
    let encoded = meta.to_json_string().unwrap();

    let identity_candidate =
        AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap();
    assert_eq!(identity_candidate.identity(), meta.identity());
    let candidate = identity_candidate.admit::<TestSettings>().unwrap();

    assert_eq!(candidate.into_meta(), meta);
    assert!(encoded.contains("\"kind\": \"asset_meta\""));
    assert!(encoded.contains("\"format_version\": 1"));
    assert!(encoded.contains("\"asset\""));
    assert!(encoded.contains("\"settings\""));
}

#[test]
fn asset_meta_rejects_the_superseded_flat_payload() {
    let encoded = sample_meta().to_json_string().unwrap();
    let mut value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    let asset = value["payload"]["asset"].take();
    let object = value["payload"].as_object_mut().unwrap();
    object.remove("settings");
    object.extend(asset.as_object().unwrap().clone());

    let flat = serde_json::to_vec(&value).unwrap();

    assert!(AssetMetaIdentityCandidate::decode_json_bytes(&flat).is_err());
    assert!(AssetMetaCandidate::<TestSettings>::decode_json_bytes(&flat).is_err());
}

#[test]
fn typed_asset_meta_rejects_unknown_settings_fields() {
    let encoded = sample_meta().to_json_string().unwrap();
    let mut value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    value["payload"]["settings"]["unexpected"] =
        serde_json::Value::String("raw-settings-canary".to_owned());
    let hostile = serde_json::to_vec(&value).unwrap();

    let identity = AssetMetaIdentityCandidate::decode_json_bytes(&hostile).unwrap();
    assert!(!format!("{identity:?}").contains("raw-settings-canary"));
    assert!(identity.admit::<TestSettings>().is_err());
}

#[test]
fn asset_meta_rejects_unknown_fields_and_the_encoded_sentinel_byte() {
    let encoded = sample_meta().to_json_string().unwrap();
    let mut value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    value["payload"]["unexpected"] = serde_json::Value::Bool(true);
    let hostile = serde_json::to_vec(&value).unwrap();
    assert!(AssetMetaIdentityCandidate::decode_json_bytes(&hostile).is_err());

    let exact =
        AssetMetaFileLimits::default().with_encoded_bytes(ByteLimit::new(encoded.len()).unwrap());
    assert!(
        AssetMetaCandidate::<TestSettings>::decode_json_bytes_with_limits(
            encoded.as_bytes(),
            exact
        )
        .is_ok()
    );
    let short = AssetMetaFileLimits::default()
        .with_encoded_bytes(ByteLimit::new(encoded.len() - 1).unwrap());
    assert!(
        AssetMetaCandidate::<TestSettings>::decode_json_bytes_with_limits(
            encoded.as_bytes(),
            short
        )
        .is_err()
    );
}

#[test]
fn asset_meta_writer_enforces_the_same_shape_limits_as_the_decoder() {
    let meta = sample_meta();
    let (identity, settings) = meta.into_parts();
    let meta = AssetMeta::new(
        AssetMetaIdentity::new(
            identity.stable_id(),
            identity.path().clone(),
            AssetSourceKind::Other("x".repeat(8 * 1024 + 1)),
        ),
        settings,
    );

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
    let decoded = AssetMetaCandidate::<TestSettings>::decode_json_bytes_with_limits(
        encoded.as_bytes(),
        widened,
    )
    .unwrap()
    .into_meta();

    assert_eq!(decoded, meta);
}

#[test]
fn stable_asset_id_random_generation_is_fallible_and_produces_distinct_ids() {
    let first = StableAssetId::try_random().unwrap();
    let second = StableAssetId::try_random().unwrap();

    assert_ne!(first, second);
    for id in [first, second] {
        assert_eq!(id.as_uuid().get_version(), Some(Version::Random));
        assert_eq!(id.as_uuid().get_variant(), Variant::RFC4122);
    }
}

fn sample_meta() -> AssetMeta<TestSettings> {
    AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str("2f0d71c7-14fc-4ed4-b48b-1c61bba8b97f").unwrap(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
        ),
        TestSettings { linear: true },
    )
}
