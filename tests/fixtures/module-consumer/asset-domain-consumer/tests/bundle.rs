use std::{mem::size_of, sync::Arc};

use nara_asset::{
    AssetImportError, AssetImportSource, AssetMeta, AssetMetaIdentity, AssetMetaIdentityCandidate,
    AssetPath, AssetProductRef, AssetSourceKind, ImportedProductId, RuntimeAssetCodec,
    SourceAssetImporter, SourceHash, StableAssetId,
};
use nara_asset_domain_consumer::{
    TextBundle, TextBundleContinuity, TextBundleDomain, TextBundleEntry,
    TextBundleIdentityDecision, TextBundleRebindAssignment, TextBundleRowAssignment,
    TextBundleSettings, TextDomain,
    bundle::{
        MAX_BUNDLE_CHARACTERS, MAX_BUNDLE_ENTRIES, MAX_BUNDLE_RETIRED, MAX_BUNDLE_SOURCE_BYTES,
        MAX_BUNDLE_SUFFIX_BYTES,
    },
};
use nara_core::ByteLimit;
use serde::Serialize;
use serde_json::{Value, json};

const SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const OTHER_SOURCE: &str = "af3176df-4556-45a5-9b2f-334ce47a5fbf";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;

#[derive(Debug, PartialEq, Eq)]
enum ReadFailure {
    Missing { sentinel: u32 },
    TooLarge { actual: usize, maximum: usize },
}

fn id(number: usize) -> ImportedProductId {
    ImportedProductId::parse_subproduct(&format!("00000000-0000-4000-8000-{number:012x}")).unwrap()
}

fn reference(product: ImportedProductId) -> AssetProductRef {
    AssetProductRef::new(StableAssetId::parse_str(SOURCE).unwrap(), product).unwrap()
}

fn settings(raw: &[u8]) -> TextBundleSettings {
    TextBundleSettings::initial(raw, 64, [id(1), id(2)]).unwrap()
}

fn metadata(settings: impl Serialize) -> AssetMetaIdentityCandidate {
    let identity = AssetMetaIdentity::new(
        StableAssetId::parse_str(SOURCE).unwrap(),
        AssetPath::new("bundle.json").unwrap(),
        AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
    );
    let encoded = AssetMeta::new(identity, settings).to_json_string().unwrap();
    AssetMetaIdentityCandidate::decode_json_bytes(encoded.as_bytes()).unwrap()
}

fn prepare(
    raw: &[u8],
    settings: impl Serialize,
    suffix: &[u8],
) -> Result<TextBundle, AssetImportError<ReadFailure>> {
    let metadata = metadata(settings);
    let suffix: Arc<[u8]> = Arc::from(suffix);
    let mut reader = |_: &AssetPath, _: ByteLimit| Ok(Arc::clone(&suffix));
    let mut source = AssetImportSource::new(&metadata, Arc::from(raw), &mut reader);
    TextBundleDomain::prepare(&mut source)
}

fn assert_no_reads(raw: &[u8], settings: impl Serialize, code: &str) {
    let metadata = metadata(settings);
    let mut reads = 0;
    let mut reader = |_: &AssetPath, _: ByteLimit| {
        reads += 1;
        Err(ReadFailure::Missing { sentinel: 71 })
    };
    let error = {
        let mut source = AssetImportSource::new(&metadata, Arc::from(raw), &mut reader);
        TextBundleDomain::prepare(&mut source).unwrap_err()
    };
    match error {
        AssetImportError::Domain(diagnostic) => assert_eq!(diagnostic.code().as_str(), code),
        AssetImportError::Source(error) => panic!("unexpected read: {error:?}"),
    }
    assert_eq!(reads, 0);
}

fn continuing(row: usize, product: ImportedProductId) -> TextBundleRebindAssignment {
    TextBundleRebindAssignment {
        row,
        identity: TextBundleIdentityDecision::Continue(product),
    }
}

fn introducing(row: usize, product: ImportedProductId) -> TextBundleRebindAssignment {
    TextBundleRebindAssignment {
        row,
        identity: TextBundleIdentityDecision::New(product),
    }
}

#[test]
fn distinct_source_kind_imports_two_scoped_products_and_one_bounded_suffix() {
    assert_eq!(TextDomain::SOURCE_KIND, "consumer.text");
    assert_eq!(TextBundleDomain::SOURCE_KIND, "consumer.text-bundle");
    let metadata = metadata(settings(RAW));
    let suffix: Arc<[u8]> = Arc::from(" world".as_bytes());
    let mut requests = Vec::new();
    let mut reader = |path: &AssetPath, limit: ByteLimit| {
        requests.push((path.clone(), limit.get()));
        Ok::<_, ReadFailure>(Arc::clone(&suffix))
    };
    let bundle = {
        let mut source = AssetImportSource::new(&metadata, Arc::from(RAW), &mut reader);
        TextBundleDomain::prepare(&mut source).unwrap()
    };
    assert_eq!(bundle.source(), StableAssetId::parse_str(SOURCE).unwrap());
    assert_eq!(bundle.entries().len(), 2);
    assert_eq!(bundle.get(&reference(id(1))).unwrap().text, "Hello world");
    assert_eq!(bundle.get(&reference(id(2))).unwrap().name, "Ending");
    assert_eq!(bundle.get(&reference(id(2))).unwrap().text, "Goodbye world");
    assert_eq!(
        requests,
        [(
            AssetPath::new("suffix.txt").unwrap(),
            MAX_BUNDLE_SUFFIX_BYTES
        )]
    );
    let foreign =
        AssetProductRef::new(StableAssetId::parse_str(OTHER_SOURCE).unwrap(), id(1)).unwrap();
    assert!(bundle.get(&foreign).is_none());
    assert!(bundle.get(&reference(ImportedProductId::PRIMARY)).is_none());
    assert!(bundle.get(&reference(id(3))).is_none());
}

#[test]
fn suffix_failures_keep_the_exact_callers_error() {
    let metadata = metadata(settings(RAW));
    let mut reader = |_: &AssetPath, _: ByteLimit| Err(ReadFailure::Missing { sentinel: 89 });
    let mut source = AssetImportSource::new(&metadata, Arc::from(RAW), &mut reader);
    assert!(matches!(
        TextBundleDomain::prepare(&mut source),
        Err(AssetImportError::Source(ReadFailure::Missing {
            sentinel: 89
        }))
    ));

    let mut reader = |_: &AssetPath, limit: ByteLimit| {
        Err(ReadFailure::TooLarge {
            actual: 1025,
            maximum: limit.get(),
        })
    };
    let mut source = AssetImportSource::new(&metadata, Arc::from(RAW), &mut reader);
    assert!(matches!(
        TextBundleDomain::prepare(&mut source),
        Err(AssetImportError::Source(ReadFailure::TooLarge {
            actual: 1025,
            maximum: 1024
        }))
    ));
}

#[test]
fn invalid_source_shape_size_strings_and_unknown_data_reject_before_suffix() {
    let mut invalid = vec![
        b"\xff".to_vec(),
        br#"{"suffix":"../suffix.txt","entries":[{"name":"a","text":"b"}]}"#.to_vec(),
        br#"{"suffix":"suffix.txt","entries":[]}"#.to_vec(),
        br#"{"suffix":"suffix.txt","suffix":"other.txt","entries":[]}"#.to_vec(),
        br#"{"suffix":"suffix.txt","entries":[{"name":"a","text":"b","id":"intrinsic"}]}"#.to_vec(),
        br#"{"suffix":"suffix.txt","entries":[{"name":"a","text":"b"}],"extra":0}"#.to_vec(),
        br#"{"suffix":"suffix.txt","entries":[{"name":"","text":"b"}]}"#.to_vec(),
        [RAW, b" {}"].concat(),
        vec![b' '; MAX_BUNDLE_SOURCE_BYTES + 1],
    ];
    invalid.push(
        serde_json::to_vec(&json!({"suffix":"suffix.txt", "entries":
        vec![json!({"name":"a", "text":"b"}); MAX_BUNDLE_ENTRIES + 1]}))
        .unwrap(),
    );
    invalid.push(
        serde_json::to_vec(&json!({"suffix":"suffix.txt", "entries":
        [{"name":"a".repeat(129), "text":"b"}]}))
        .unwrap(),
    );
    invalid.push(
        serde_json::to_vec(&json!({"suffix":"suffix.txt", "entries":
        [{"name":"a", "text":"b".repeat(16 * 1024 + 1)}]}))
        .unwrap(),
    );
    invalid.push(
        serde_json::to_vec(&json!({"suffix":"a".repeat(513), "entries":
        [{"name":"a", "text":"b"}]}))
        .unwrap(),
    );
    invalid.push(format!("{}0{}", "[".repeat(20), "]".repeat(20)).into_bytes());
    for raw in invalid {
        assert_no_reads(&raw, settings(RAW), "consumer.text-bundle.source-invalid");
    }
}

#[test]
fn lost_unknown_or_invalid_settings_never_authorize_suffix_reads() {
    let valid = serde_json::to_value(settings(RAW)).unwrap();
    let mut invalid = vec![
        json!({"max_characters":64}),
        json!({"max_characters":64, "continuity":null}),
        json!("missing authored settings"),
    ];
    for maximum in [0, MAX_BUNDLE_CHARACTERS + 1] {
        let mut value = valid.clone();
        value["max_characters"] = json!(maximum);
        invalid.push(value);
    }
    let mut unknown = valid.clone();
    unknown["guess_by_name"] = json!(true);
    invalid.push(unknown);
    let mut unknown = valid.clone();
    unknown["continuity"]["cache"] = json!("identity authority");
    invalid.push(unknown);
    for value in invalid {
        assert_no_reads(RAW, value, "consumer.text-bundle.settings-invalid");
    }
}

#[test]
fn duplicate_missing_primary_and_retired_mappings_reject_before_suffix() {
    let valid = serde_json::to_value(settings(RAW)).unwrap();
    let mut invalid = Vec::new();
    let mut value = valid.clone();
    value["continuity"]["assignments"][1]["product"] = json!(id(1));
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["assignments"][1]["row"] = json!(0);
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["assignments"][1]["row"] = json!(2);
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["assignments"][1]["product"] = json!(ImportedProductId::PRIMARY);
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["retired"] = json!([id(1)]);
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["retired"] = json!([id(3), id(3)]);
    invalid.push(value);
    let mut value = valid.clone();
    value["continuity"]["retired"] = json!([ImportedProductId::PRIMARY]);
    invalid.push(value);
    let mut value = valid;
    value["continuity"]["assignments"]
        .as_array_mut()
        .unwrap()
        .pop();
    invalid.push(value);
    for value in invalid {
        assert_no_reads(RAW, value, "consumer.text-bundle.continuity-invalid");
    }
}

#[test]
fn settings_sequences_reject_oversized_or_unknown_rows_during_deserialization() {
    let mut value = serde_json::to_value(settings(RAW)).unwrap();
    value["continuity"]["assignments"] = json!(
        (0..MAX_BUNDLE_ENTRIES + 1)
            .map(|row| TextBundleRowAssignment {
                row,
                product: id(row + 1)
            })
            .collect::<Vec<_>>()
    );
    assert!(serde_json::from_value::<TextBundleSettings>(value).is_err());
    let mut value = serde_json::to_value(settings(RAW)).unwrap();
    value["continuity"]["retired"] = json!(
        (10..10 + MAX_BUNDLE_RETIRED + 1)
            .map(id)
            .collect::<Vec<_>>()
    );
    assert!(serde_json::from_value::<TextBundleSettings>(value).is_err());
    let mut value = serde_json::to_value(settings(RAW)).unwrap();
    value["continuity"]["assignments"][0]["name"] = json!("heuristic");
    assert!(serde_json::from_value::<TextBundleSettings>(value).is_err());
}

#[test]
fn digest_binds_exact_source_revision_and_never_mints_product_identity() {
    let settings = settings(RAW);
    assert_eq!(
        settings.continuity.source_hash(),
        SourceHash::from_bytes(RAW)
    );
    let whitespace_change = [RAW, b"\n"].concat();
    assert_no_reads(
        &whitespace_change,
        &settings,
        "consumer.text-bundle.continuity-stale",
    );
    let rebound = settings
        .rebind(
            &whitespace_change,
            &[continuing(0, id(1)), continuing(1, id(2))],
        )
        .unwrap();
    assert_ne!(
        rebound.continuity.source_hash(),
        settings.continuity.source_hash()
    );
    assert_eq!(
        rebound.continuity.assignments(),
        settings.continuity.assignments()
    );
    assert_eq!(
        prepare(RAW, settings, b"!").unwrap(),
        prepare(&whitespace_change, rebound, b"!").unwrap()
    );
}

#[test]
fn explicit_rename_reorder_and_body_change_preserve_the_selected_products() {
    let before = settings(RAW);
    let raw = br#"{"suffix":"suffix.txt","entries":[{"name":"Credits","text":"Farewell"},{"name":"Introduction","text":"Welcome"}]}"#;
    let after = before
        .rebind(raw, &[continuing(1, id(1)), continuing(0, id(2))])
        .unwrap();
    after.validate_successor(&before).unwrap();
    assert!(after.continuity.retired().is_empty());
    let cooked = prepare(raw, &after, b"!").unwrap();
    assert_eq!(cooked.get(&reference(id(1))).unwrap().name, "Introduction");
    assert_eq!(cooked.get(&reference(id(1))).unwrap().text, "Welcome!");
    assert_eq!(cooked.get(&reference(id(2))).unwrap().name, "Credits");
    assert_eq!(cooked.get(&reference(id(2))).unwrap().text, "Farewell!");
    assert_eq!(after.continuity.assignments()[0].product, id(2));
}

#[test]
fn equal_byte_rows_still_require_explicit_authority_and_are_not_claimed_detectable() {
    let raw = br#"{"suffix":"suffix.txt","entries":[{"name":"same","text":"same"},{"name":"same","text":"same"}]}"#;
    assert!(TextBundleSettings::initial(raw, 64, []).is_err());
    let before = TextBundleSettings::initial(raw, 64, [id(1), id(2)]).unwrap();
    let after = before
        .rebind(raw, &[continuing(0, id(2)), continuing(1, id(1))])
        .unwrap();
    assert_eq!(
        before.continuity.source_hash(),
        after.continuity.source_hash()
    );
    assert_ne!(
        before.continuity.assignments(),
        after.continuity.assignments()
    );
    assert_eq!(
        prepare(raw, &before, b"!").unwrap(),
        prepare(raw, &after, b"!").unwrap()
    );
    assert_no_reads(
        raw,
        json!({"max_characters":64}),
        "consumer.text-bundle.settings-invalid",
    );
}

#[test]
fn rebind_requires_explicit_valid_continue_and_new_decisions() {
    let before = settings(RAW);
    for decisions in [
        vec![continuing(0, id(1))],
        vec![continuing(0, id(1)), continuing(0, id(2))],
        vec![continuing(0, id(1)), continuing(1, id(1))],
        vec![continuing(0, id(1)), continuing(2, id(2))],
        vec![continuing(0, id(1)), continuing(1, id(3))],
        vec![continuing(0, id(1)), introducing(1, id(2))],
        vec![
            continuing(0, id(1)),
            introducing(1, ImportedProductId::PRIMARY),
        ],
    ] {
        assert!(before.rebind(RAW, &decisions).is_err());
    }
    let after = before
        .rebind(RAW, &[continuing(0, id(1)), introducing(1, id(3))])
        .unwrap();
    assert_eq!(after.continuity.retired(), &[id(2)]);
    for decision in [continuing(1, id(2)), introducing(1, id(2))] {
        assert!(
            after
                .rebind(RAW, &[continuing(0, id(1)), decision])
                .is_err()
        );
    }
    assert_eq!(before.continuity.retired(), &[]);
}

#[test]
fn successor_hook_preserves_retirements_and_rejects_lost_or_reused_history() {
    let before = settings(RAW);
    let next = before
        .rebind(RAW, &[continuing(0, id(1)), introducing(1, id(3))])
        .unwrap();
    next.validate_successor(&before).unwrap();
    assert!(
        TextBundleSettings::initial(RAW, 64, [id(1), id(3)])
            .unwrap()
            .validate_successor(&before)
            .is_err()
    );
    let third = next
        .rebind(RAW, &[introducing(0, id(4)), continuing(1, id(3))])
        .unwrap();
    assert_eq!(third.continuity.retired(), &[id(1), id(2)]);
    third.validate_successor(&next).unwrap();
    let mut lost = serde_json::to_value(&third).unwrap();
    lost["continuity"]["retired"] = json!([id(1)]);
    let lost: TextBundleSettings = serde_json::from_value(lost).unwrap();
    assert!(lost.validate_successor(&next).is_err());
    let reused = TextBundleSettings::initial(RAW, 64, [id(2), id(3)]).unwrap();
    assert!(reused.validate_successor(&next).is_err());
}

#[test]
fn retirement_record_is_bounded_and_never_silently_evicts_old_ids() {
    let raw = serde_json::to_vec(&json!({"suffix":"suffix.txt", "entries":
        vec![json!({"name":"a", "text":"b"}); MAX_BUNDLE_ENTRIES]}))
    .unwrap();
    let mut current =
        TextBundleSettings::initial(&raw, 64, (1..=MAX_BUNDLE_ENTRIES).map(id)).unwrap();
    for generation in 1..=2 {
        let decisions: Vec<_> = (0..MAX_BUNDLE_ENTRIES)
            .map(|row| introducing(row, id(generation * MAX_BUNDLE_ENTRIES + row + 1)))
            .collect();
        current = current.rebind(&raw, &decisions).unwrap();
    }
    assert_eq!(current.continuity.retired().len(), MAX_BUNDLE_RETIRED);
    // Full-size live and retirement sets still fit ordinary AssetMeta admission limits.
    let cooked = prepare(&raw, &current, b"!").unwrap();
    assert_eq!(cooked.entries().len(), MAX_BUNDLE_ENTRIES);
    let decisions: Vec<_> = (0..MAX_BUNDLE_ENTRIES)
        .map(|row| introducing(row, id(100 + row)))
        .collect();
    let error = current.rebind(&raw, &decisions).unwrap_err();
    assert_eq!(
        error.diagnostic().code().as_str(),
        "consumer.text-bundle.retirement-limit"
    );
    assert_eq!(current.continuity.retired().len(), MAX_BUNDLE_RETIRED);
}

#[test]
fn initial_authorship_is_complete_bounded_and_independent_of_assignment_order() {
    let initial = TextBundleContinuity::author_initial(
        RAW,
        &[
            TextBundleRowAssignment {
                row: 1,
                product: id(2),
            },
            TextBundleRowAssignment {
                row: 0,
                product: id(1),
            },
        ],
    )
    .unwrap();
    assert_eq!(initial, settings(RAW).continuity);
    for products in [
        vec![id(1)],
        vec![id(1), id(1)],
        vec![ImportedProductId::PRIMARY, id(2)],
    ] {
        assert!(TextBundleSettings::initial(RAW, 64, products).is_err());
    }
    assert!(TextBundleSettings::initial(RAW, 64, (1..).map(id)).is_err());
    assert!(TextBundleSettings::initial(RAW, 0, [id(1), id(2)]).is_err());
}

#[test]
fn suffix_utf8_bytes_and_cooked_character_limits_are_checked() {
    let raw = br#"{"suffix":"suffix.txt","entries":[{"name":"a","text":"Hi"}]}"#;
    let exact = TextBundleSettings::initial(raw, 4, [id(1)]).unwrap();
    let bundle = prepare(raw, &exact, "世界".as_bytes()).unwrap();
    assert_eq!(bundle.get(&reference(id(1))).unwrap().text, "Hi世界");
    for suffix in [b"\xff".to_vec(), vec![b'x'; MAX_BUNDLE_SUFFIX_BYTES + 1]] {
        assert!(matches!(
            prepare(raw, &exact, &suffix),
            Err(AssetImportError::Domain(_))
        ));
    }
    assert!(matches!(
        prepare(raw, &exact, b"123"),
        Err(AssetImportError::Domain(_))
    ));
    let tiny = TextBundleSettings::initial(raw, 1, [id(1)]).unwrap();
    assert_no_reads(raw, tiny, "consumer.text-bundle.too-long");
}

#[test]
fn runtime_is_deterministic_cooked_only_and_independent_of_authoring_inputs() {
    let (encoded, expected) = {
        let raw = RAW.to_vec();
        let settings = TextBundleSettings::initial(&raw, 64, [id(2), id(1)]).unwrap();
        let bundle = prepare(&raw, &settings, b" world").unwrap();
        let first = TextBundleDomain::encode_runtime(&bundle).unwrap();
        assert_eq!(first, TextBundleDomain::encode_runtime(&bundle).unwrap());
        let wire: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(wire["kind"], "consumer_text_bundle");
        assert_eq!(wire["format_version"], 1);
        assert_eq!(wire["engine_min_version"], "0.1.0");
        assert_eq!(
            wire["payload"]["entries"][0]["reference"]["product"],
            json!(id(1))
        );
        let text = std::str::from_utf8(&first).unwrap();
        for authoring in [
            "suffix.txt",
            "max_characters",
            "continuity",
            "source_hash",
            "retired",
        ] {
            assert!(!text.contains(authoring));
        }
        (first, bundle)
    };
    let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(decoded.get(&reference(id(2))).unwrap().text, "Hello world");
    assert_eq!(TextBundleDomain::encode_runtime(&decoded).unwrap(), encoded);
    assert_eq!(
        <TextBundleDomain as RuntimeAssetCodec<TextBundle>>::RUNTIME_VERSION,
        1
    );
    assert!(TextBundleDomain::decode_runtime(RAW).is_err());
}

#[test]
fn runtime_retained_cost_counts_inline_map_entries_and_cooked_strings() {
    let bundle = prepare(RAW, settings(RAW), b" world").unwrap();
    let encoded = TextBundleDomain::encode_runtime(&bundle).unwrap();
    let expected = size_of::<TextBundle>()
        + 2 * (size_of::<(ImportedProductId, TextBundleEntry)>() + 10 * size_of::<usize>())
        + "Opening".len()
        + "Hello world".len()
        + "Ending".len()
        + "Goodbye world".len();
    assert_eq!(
        TextBundleDomain::runtime_retained_bytes(&bundle).unwrap(),
        expected
    );
    let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
    assert_eq!(
        TextBundleDomain::runtime_retained_bytes(&decoded).unwrap(),
        expected
    );
    assert!(expected < TextBundleDomain::runtime_limits().retained_bytes().get());
    assert_eq!(TextBundleDomain::encode_runtime(&bundle).unwrap(), encoded);
    assert_eq!(decoded, bundle);
}

#[test]
fn runtime_retained_cost_depends_on_cooked_bytes_instead_of_json_escaping() {
    let mut observations = Vec::new();
    for text in ["x".repeat(64), "\0".repeat(64)] {
        let raw = serde_json::to_vec(&json!({
            "suffix": "suffix.txt",
            "entries": [{"name": "entry", "text": text}],
        }))
        .unwrap();
        let settings = TextBundleSettings::initial(&raw, 64, [id(1)]).unwrap();
        let bundle = prepare(&raw, settings, b"").unwrap();
        let encoded = TextBundleDomain::encode_runtime(&bundle).unwrap();
        let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
        observations.push((
            encoded.len(),
            TextBundleDomain::runtime_retained_bytes(&decoded).unwrap(),
        ));
    }
    assert!(observations[1].0 > observations[0].0);
    assert_eq!(observations[0].1, observations[1].1);
}

fn runtime_wire() -> Value {
    let bundle = prepare(RAW, settings(RAW), b"!").unwrap();
    serde_json::from_slice(&TextBundleDomain::encode_runtime(&bundle).unwrap()).unwrap()
}

fn reject_runtime(wire: Value) {
    let error = TextBundleDomain::decode_runtime(&serde_json::to_vec(&wire).unwrap()).unwrap_err();
    assert_eq!(
        error.diagnostic().code().as_str(),
        "consumer.text-bundle.runtime-invalid"
    );
}

#[test]
fn runtime_rejects_wrong_kind_version_engine_schema_and_authoring_fallback_data() {
    let valid = runtime_wire();
    for (key, value) in [
        ("kind", json!("consumer_text")),
        ("format_version", json!(2)),
        ("engine_min_version", json!("0.2.0")),
        (
            "raw_source",
            json!(String::from_utf8(RAW.to_vec()).unwrap()),
        ),
    ] {
        let mut wire = valid.clone();
        wire[key] = value;
        reject_runtime(wire);
    }
    for key in ["source", "entries"] {
        let mut wire = valid.clone();
        wire["payload"].as_object_mut().unwrap().remove(key);
        reject_runtime(wire);
    }
    for key in ["continuity", "metadata", "suffix", "unknown"] {
        let mut wire = valid.clone();
        wire["payload"][key] = json!("forbidden");
        reject_runtime(wire);
    }
    for key in ["raw_text", "unknown"] {
        let mut wire = valid.clone();
        wire["payload"]["entries"][0][key] = json!("forbidden");
        reject_runtime(wire);
    }
    let mut wire = valid;
    wire["payload"]["entries"][0]["reference"]["extra"] = json!(0);
    reject_runtime(wire);
}

#[test]
fn runtime_rejects_duplicate_primary_unsorted_and_wrong_enclosing_identities() {
    let valid = runtime_wire();
    let mut wire = valid.clone();
    wire["payload"]["entries"][1]["reference"] = wire["payload"]["entries"][0]["reference"].clone();
    reject_runtime(wire);
    let mut wire = valid.clone();
    wire["payload"]["entries"].as_array_mut().unwrap().reverse();
    reject_runtime(wire);
    let mut wire = valid.clone();
    wire["payload"]["entries"][0]["reference"]["product"] = json!(ImportedProductId::PRIMARY);
    reject_runtime(wire);
    let mut wire = valid.clone();
    wire["payload"]["entries"][0]["reference"]["source"] = json!(OTHER_SOURCE);
    reject_runtime(wire);
    let mut wire = valid.clone();
    wire["payload"]["source"] = json!("00000000-0000-0000-0000-000000000000");
    reject_runtime(wire);
    let mut wire = valid;
    wire["payload"]["entries"][0]["reference"]["product"]["id"] =
        json!("00000000-0000-0000-0000-000000000000");
    reject_runtime(wire);
}

#[test]
fn runtime_rejects_oversize_shape_strings_duplicate_keys_and_trailing_data() {
    let limits = <TextBundleDomain as RuntimeAssetCodec<TextBundle>>::runtime_limits();
    assert!(
        TextBundleDomain::decode_runtime(&vec![b' '; limits.encoded_bytes().get() + 1]).is_err()
    );
    let valid = runtime_wire();
    let mut wire = valid.clone();
    wire["payload"]["entries"] = json!([]);
    reject_runtime(wire);
    let mut wire = valid.clone();
    wire["payload"]["entries"] = json!(vec![
        valid["payload"]["entries"][0].clone();
        MAX_BUNDLE_ENTRIES + 1
    ]);
    reject_runtime(wire);
    for (key, text) in [
        ("name", "x".repeat(129)),
        ("text", "x".repeat(MAX_BUNDLE_CHARACTERS + 1)),
        ("text", "x".repeat(16 * 1024 + 1)),
    ] {
        let mut wire = valid.clone();
        wire["payload"]["entries"][0][key] = json!(text);
        reject_runtime(wire);
    }
    let encoded = serde_json::to_string(&valid).unwrap();
    let duplicate = encoded.replacen("{", "{\"kind\":\"consumer_text_bundle\",", 1);
    assert!(TextBundleDomain::decode_runtime(duplicate.as_bytes()).is_err());
    assert!(TextBundleDomain::decode_runtime(format!("{encoded} {{}}").as_bytes()).is_err());
    let deep = format!("{}0{}", "[".repeat(20), "]".repeat(20));
    assert!(TextBundleDomain::decode_runtime(deep.as_bytes()).is_err());
}

#[test]
fn maximum_multibyte_and_escaped_text_round_trip_through_the_runtime_format() {
    for (body, suffix) in [
        ("\u{1f642}".repeat(3840), "\u{1f642}".repeat(256)),
        ("\0".repeat(3840), "\0".repeat(256)),
    ] {
        let raw = serde_json::to_vec(&json!({
            "suffix": "suffix.txt",
            "entries": [{"name": "boundary", "text": body}],
        }))
        .unwrap();
        let settings = TextBundleSettings::initial(&raw, MAX_BUNDLE_CHARACTERS, [id(1)]).unwrap();
        let product = prepare(&raw, settings, suffix.as_bytes()).unwrap();
        let encoded = TextBundleDomain::encode_runtime(&product).unwrap();
        assert_eq!(
            product.entries()[&id(1)].text.chars().count(),
            MAX_BUNDLE_CHARACTERS
        );
        let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
        let retained = TextBundleDomain::runtime_retained_bytes(&decoded).unwrap();
        assert_eq!(
            retained,
            size_of::<TextBundle>()
                + size_of::<(ImportedProductId, TextBundleEntry)>()
                + 10 * size_of::<usize>()
                + "boundary".len()
                + product.entries()[&id(1)].text.len(),
        );
        assert!(retained <= TextBundleDomain::runtime_limits().retained_bytes().get());
        assert_eq!(decoded, product);
    }
}
