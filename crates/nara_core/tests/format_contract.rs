#![cfg(feature = "serde")]

use std::sync::atomic::{AtomicUsize, Ordering};

use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, FormatVersion, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeError, SerdeShapeLimits,
    decode_persistent_file, decode_persistent_file_with_preflight, preflight_serde_shape,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    value: u32,
}

static PAYLOAD_DECODE_COUNT: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct CountedPayload {
    value: u32,
}

impl<'de> serde::Deserialize<'de> for CountedPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            value: u32,
        }

        PAYLOAD_DECODE_COUNT.fetch_add(1, Ordering::SeqCst);
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self { value: wire.value })
    }
}

fn scene_contract(current_engine: &str) -> PersistentFileContract {
    PersistentFileContract::canonical_v1(
        FormatKind::new("scene").unwrap(),
        EngineVersion::parse(current_engine).unwrap(),
    )
}

fn decode_counted_json(
    input: &str,
    maximum_bytes: usize,
    contract: &PersistentFileContract,
) -> Result<PersistentFileEnvelope<CountedPayload>, PersistentFileDecodeError<serde_json::Error>> {
    decode_persistent_file(
        input.as_bytes(),
        ByteLimit::new(maximum_bytes).unwrap(),
        contract,
        |encoded| serde_json::from_slice::<PersistentFileHeader>(encoded),
        |encoded| serde_json::from_slice::<PersistentFileEnvelope<CountedPayload>>(encoded),
    )
}

#[test]
fn persistent_file_envelope_has_one_strict_canonical_shape() {
    let envelope = PersistentFileEnvelope::canonical_v1(
        FormatKind::new("scene").unwrap(),
        EngineVersion::parse("0.1.0").unwrap(),
        FormatGenerator::new("nara", EngineVersion::parse("0.1.0").unwrap()).unwrap(),
        Payload { value: 7 },
    );

    let encoded = serde_json::to_string(&envelope).unwrap();
    assert_eq!(
        encoded,
        r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"value":7}}"#
    );

    let decoded: PersistentFileEnvelope<Payload> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, envelope);
    assert_eq!(decoded.kind().as_str(), "scene");
    assert_eq!(decoded.format_version().get(), 1);
    assert_eq!(decoded.engine_min_version().to_string(), "0.1.0");
    assert_eq!(decoded.generator().name(), "nara");
    assert_eq!(decoded.payload(), &Payload { value: 7 });
    assert_eq!(decoded.into_payload(), Payload { value: 7 });
}

#[test]
fn format_values_reject_invalid_or_unbounded_identifiers() {
    assert!(FormatKind::new("").is_err());
    assert!(FormatKind::new("Scene").is_err());
    assert!(FormatKind::new("scene-patch").is_err());
    assert!(FormatKind::new("scene_patch").is_ok());
    assert!(FormatKind::new("x".repeat(FormatKind::MAX_BYTES)).is_ok());
    assert!(FormatKind::new("x".repeat(FormatKind::MAX_BYTES + 1)).is_err());

    assert!(FormatGenerator::new("", EngineVersion::parse("0.1.0").unwrap()).is_err());
    assert!(FormatGenerator::new("nara tool", EngineVersion::parse("0.1.0").unwrap()).is_err());
    assert!(
        FormatGenerator::new(
            "x".repeat(FormatGenerator::MAX_NAME_BYTES),
            EngineVersion::parse("0.1.0").unwrap(),
        )
        .is_ok()
    );
    assert!(
        FormatGenerator::new(
            "x".repeat(FormatGenerator::MAX_NAME_BYTES + 1),
            EngineVersion::parse("0.1.0").unwrap(),
        )
        .is_err()
    );
}

#[test]
fn format_and_engine_versions_preserve_their_invariants() {
    assert_eq!(FormatVersion::new(0), None);
    assert_eq!(FormatVersion::new(1).map(FormatVersion::get), Some(1));
    assert!(EngineVersion::parse("not-a-version").is_err());
    assert!(EngineVersion::parse("0.1.0").unwrap() < EngineVersion::parse("0.2.0").unwrap());
    assert!(
        EngineVersion::parse("1.2.3+local.1")
            .unwrap()
            .meets_minimum(&EngineVersion::parse("1.2.3+release.99").unwrap())
    );

    assert!(serde_json::from_str::<FormatVersion>("0").is_err());
    assert!(serde_json::from_str::<EngineVersion>(r#""invalid""#).is_err());
}

#[test]
fn envelope_and_generator_reject_unknown_fields() {
    let unknown_envelope = r#"{
        "kind":"scene",
        "format_version":1,
        "engine_min_version":"0.1.0",
        "generator":{"name":"nara","version":"0.1.0"},
        "payload":{"value":7},
        "unexpected":true
    }"#;
    assert!(serde_json::from_str::<PersistentFileEnvelope<Payload>>(unknown_envelope).is_err());

    let unknown_generator = r#"{
        "kind":"scene",
        "format_version":1,
        "engine_min_version":"0.1.0",
        "generator":{"name":"nara","version":"0.1.0","unexpected":true},
        "payload":{"value":7}
    }"#;
    assert!(serde_json::from_str::<PersistentFileEnvelope<Payload>>(unknown_generator).is_err());
}

#[test]
fn encoded_byte_budget_rejects_before_header_or_payload_decode() {
    PAYLOAD_DECODE_COUNT.store(0, Ordering::SeqCst);
    let input = r#"{
        "payload":{"value":7},
        "kind":"scene",
        "format_version":1,
        "engine_min_version":"0.1.0",
        "generator":{"name":"nara","version":"0.1.0"}
    }"#;

    let error = decode_counted_json(input, input.len() - 1, &scene_contract("0.1.0"))
        .expect_err("oversized input must fail");

    assert!(matches!(
        error,
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum }
            if observed == input.len() && maximum == input.len() - 1
    ));
    assert_eq!(PAYLOAD_DECODE_COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn shape_preflight_rejects_before_header_or_payload_decode() {
    PAYLOAD_DECODE_COUNT.store(0, Ordering::SeqCst);
    let input = r#"{"kind":"scene","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"},"payload":{"value":7}}"#;
    let mut header_decoded = false;

    let error = decode_persistent_file_with_preflight(
        input.as_bytes(),
        ByteLimit::new(input.len()).unwrap(),
        &scene_contract("0.1.0"),
        |_| Err(serde_json::Error::io(std::io::Error::other("shape limit"))),
        |_| {
            header_decoded = true;
            serde_json::from_str::<PersistentFileHeader>(input)
        },
        |_| serde_json::from_str::<PersistentFileEnvelope<CountedPayload>>(input),
    )
    .expect_err("preflight failure must reject");

    assert!(matches!(error, PersistentFileDecodeError::Shape(_)));
    assert!(!header_decoded);
    assert_eq!(PAYLOAD_DECODE_COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn persistent_envelope_maps_payload_without_changing_header() {
    let envelope = PersistentFileEnvelope::canonical_v1(
        FormatKind::new("scene").unwrap(),
        EngineVersion::parse("0.1.0").unwrap(),
        FormatGenerator::new("nara", EngineVersion::parse("0.1.0").unwrap()).unwrap(),
        Payload { value: 7 },
    );

    let mapped = envelope.map_payload(|payload| payload.value);

    assert_eq!(mapped.kind().as_str(), "scene");
    assert_eq!(mapped.format_version(), FormatVersion::ONE);
    assert_eq!(mapped.engine_min_version().to_string(), "0.1.0");
    assert_eq!(mapped.generator().name(), "nara");
    assert_eq!(mapped.into_payload(), 7);
}

fn shape_limits(depth: usize, nodes: usize, items: usize, string_bytes: usize) -> SerdeShapeLimits {
    SerdeShapeLimits::new(
        DepthLimit::new(depth).unwrap(),
        ItemLimit::new(nodes).unwrap(),
        ItemLimit::new(items).unwrap(),
        ByteLimit::new(string_bytes).unwrap(),
        ByteLimit::new(string_bytes).unwrap(),
    )
}

#[test]
fn serde_shape_preflight_rejects_deep_duplicate_and_oversized_values_without_payload_decode() {
    let deep = r#"{"a":{"b":{"c":1}}}"#;
    let mut deep_error = serde_json::Deserializer::from_str(deep);
    let deep_error = preflight_serde_shape(&mut deep_error, shape_limits(3, 32, 32, 128))
        .expect_err("depth must be limited");
    assert!(deep_error.to_string().contains("nesting"));

    let duplicate = r#"{"a":1,"a":2}"#;
    let mut duplicate_error = serde_json::Deserializer::from_str(duplicate);
    let duplicate_error = preflight_serde_shape(&mut duplicate_error, shape_limits(8, 32, 32, 128))
        .expect_err("duplicate map keys must be rejected");
    assert!(duplicate_error.to_string().contains("duplicate"));

    let long_string = r#"{"a":"too-long"}"#;
    let mut string_error = serde_json::Deserializer::from_str(long_string);
    let string_error = preflight_serde_shape(&mut string_error, shape_limits(8, 32, 32, 3))
        .expect_err("strings must be limited");
    assert!(string_error.to_string().contains("string"));

    assert_eq!(
        SerdeShapeError::DuplicateMapKey.to_string(),
        "persistent data contains a duplicate map key"
    );
}

#[test]
fn incompatible_header_rejects_before_payload_decode_even_when_payload_comes_first() {
    let cases = [
        (
            r#"{"payload":{"value":7},"kind":"prefab","format_version":1,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"}}"#,
            "kind",
        ),
        (
            r#"{"payload":{"value":7},"kind":"scene","format_version":2,"engine_min_version":"0.1.0","generator":{"name":"nara","version":"0.1.0"}}"#,
            "version",
        ),
        (
            r#"{"payload":{"value":7},"kind":"scene","format_version":1,"engine_min_version":"0.2.0","generator":{"name":"nara","version":"0.2.0"}}"#,
            "engine",
        ),
    ];

    for (input, expected_error) in cases {
        PAYLOAD_DECODE_COUNT.store(0, Ordering::SeqCst);
        let error = decode_counted_json(input, input.len(), &scene_contract("0.1.0"))
            .expect_err("incompatible header must fail");
        assert_eq!(PAYLOAD_DECODE_COUNT.load(Ordering::SeqCst), 0);
        match expected_error {
            "kind" => assert!(matches!(
                error,
                PersistentFileDecodeError::Contract(PersistentFileContractError::WrongKind { .. })
            )),
            "version" => assert!(matches!(
                error,
                PersistentFileDecodeError::Contract(
                    PersistentFileContractError::UnsupportedFormatVersion { .. }
                )
            )),
            "engine" => assert!(matches!(
                error,
                PersistentFileDecodeError::Contract(
                    PersistentFileContractError::EngineVersionTooOld { .. }
                )
            )),
            _ => unreachable!(),
        }
    }
}

#[test]
fn compatible_header_allows_payload_decode_exactly_once() {
    PAYLOAD_DECODE_COUNT.store(0, Ordering::SeqCst);
    let input = r#"{"payload":{"value":7},"kind":"scene","format_version":1,"engine_min_version":"0.1.0+release.8","generator":{"name":"nara","version":"0.1.0"}}"#;

    let decoded = decode_counted_json(input, input.len(), &scene_contract("0.1.0+local.2"))
        .expect("compatible input should decode");

    assert_eq!(decoded.payload(), &CountedPayload { value: 7 });
    assert_eq!(PAYLOAD_DECODE_COUNT.load(Ordering::SeqCst), 1);
}
