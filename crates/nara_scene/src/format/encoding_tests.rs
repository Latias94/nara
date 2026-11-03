use std::cell::RefCell;

use nara_identity::ReferenceInputId;
use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue};

use super::*;

#[test]
fn source_and_prefab_string_apis_preserve_the_canonical_pretty_output() {
    let source = source();
    let prefab = prefab(&source);
    let original_source = source.clone();
    let original_prefab = prefab.clone();

    for (actual, expected) in [
        (
            source.to_json_string().unwrap(),
            reference(&source, SceneFileEncoding::Json),
        ),
        (
            source.to_ron_string().unwrap(),
            reference(&source, SceneFileEncoding::Ron),
        ),
        (
            prefab.to_json_string().unwrap(),
            reference(&prefab, SceneFileEncoding::Json),
        ),
        (
            prefab.to_ron_string().unwrap(),
            reference(&prefab, SceneFileEncoding::Ron),
        ),
    ] {
        assert_eq!(actual, expected);
        assert!(!actual.contains('\r'));
    }
    assert_eq!(source, original_source);
    assert_eq!(prefab, original_prefab);
}

#[test]
fn each_document_encoder_accepts_exact_output_and_refuses_one_byte_short() {
    let source = source();
    let prefab = prefab(&source);
    let patch = ScenePatchDocument::new(
        source
            .entities
            .iter()
            .cloned()
            .map(|entity| ScenePatchOperation::AddEntity { entity })
            .chain(std::iter::once(ScenePatchOperation::SetPlacements {
                expected: Vec::new(),
                value: source.placements.clone(),
            })),
    );

    assert_encoding_boundary(
        &source,
        SceneFileEncoding::Json,
        SceneDocument::to_json_string_with_limits,
    );
    assert_encoding_boundary(
        &source,
        SceneFileEncoding::Ron,
        SceneDocument::to_ron_string_with_limits,
    );
    assert_encoding_boundary(
        &prefab,
        SceneFileEncoding::Json,
        PrefabDocument::to_json_string_with_limits,
    );
    assert_encoding_boundary(
        &prefab,
        SceneFileEncoding::Ron,
        PrefabDocument::to_ron_string_with_limits,
    );
    assert_encoding_boundary(
        &patch,
        SceneFileEncoding::Json,
        ScenePatchDocument::to_json_string_with_limits,
    );
    assert_encoding_boundary(
        &patch,
        SceneFileEncoding::Ron,
        ScenePatchDocument::to_ron_string_with_limits,
    );
}

#[test]
fn source_and_prefab_encoding_preserve_the_original_allocation_refusal() {
    let source = source();
    let prefab = prefab(&source);
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        for prefab_kind in [false, true] {
            for threshold in [1, 4096] {
                let mut refusal = Some(Box::new(73_usize));
                let original = std::ptr::from_ref(refusal.as_deref().unwrap());
                let mut rejected_capacity = None;
                let mut reject = |capacity| {
                    if capacity < threshold {
                        return Ok(());
                    }
                    rejected_capacity = Some(capacity);
                    Err(refusal.take().unwrap())
                };
                let result = if prefab_kind {
                    prefab.encode_with_admission(encoding, SceneFileLimits::default(), &mut reject)
                } else {
                    source.encode_with_admission(encoding, SceneFileLimits::default(), &mut reject)
                };
                assert!(rejected_capacity.is_some_and(|capacity| capacity >= threshold));
                match result {
                    Err(SceneFileEncodingAdmissionError::Admission(error)) => {
                        assert_eq!(std::ptr::from_ref(error.as_ref()), original);
                    }
                    other => panic!("allocation refusal lost: {other:?}"),
                }
            }
        }
    }
}

#[test]
fn source_and_prefab_admitted_encoding_keeps_output_and_file_limit_errors() {
    let source = source();
    let prefab = prefab(&source);
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        for prefab_kind in [false, true] {
            let expected = if prefab_kind {
                reference(&prefab, encoding)
            } else {
                reference(&source, encoding)
            };
            for maximum in [expected.len(), expected.len() - 1] {
                let limits =
                    SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(maximum).unwrap());
                let mut allocations = Vec::new();
                let mut admit = |capacity| {
                    assert!(capacity <= maximum);
                    allocations.push(capacity);
                    Ok::<_, std::convert::Infallible>(())
                };
                let result = if prefab_kind {
                    prefab.encode_with_admission(encoding, limits, &mut admit)
                } else {
                    source.encode_with_admission(encoding, limits, &mut admit)
                };
                assert!(!allocations.is_empty());
                // The large string exercises many small RON writes. Admission follows
                // allocations rather than the serializer's per-character writes.
                assert!(allocations.len() < 128);
                if maximum == expected.len() {
                    assert_eq!(result.unwrap(), expected);
                } else {
                    assert!(matches!(
                        result,
                        Err(SceneFileEncodingAdmissionError::Format(
                            SceneFormatError::EncodedBytesExceeded { observed, maximum: limit }
                        )) if observed == expected.len() && limit == maximum
                    ));
                }
            }
        }
    }
}

#[test]
fn bounded_writer_admits_full_capacity_only_before_growth() {
    let allocations = RefCell::new(Vec::new());
    let mut admit = |capacity| {
        allocations.borrow_mut().push(capacity);
        Ok(())
    };
    let mut encoded = BoundedEncodedBuffer::new(7, true, Some(&mut admit));
    assert_eq!(encoded.bytes.capacity(), 0);
    assert!(allocations.borrow().is_empty());

    encoded.write_all(b"abc").unwrap();
    assert_eq!(*allocations.borrow(), [3]);
    encoded.write_all(b"de").unwrap();
    assert_eq!(*allocations.borrow(), [3, 6]);
    encoded.write_all(b"f").unwrap();
    assert_eq!(*allocations.borrow(), [3, 6]);
    encoded.write_all(b"g").unwrap();
    assert_eq!(*allocations.borrow(), [3, 6, 7]);
    assert_eq!(encoded.bytes.capacity(), 7);

    assert!(encoded.write_all(b"overflow").is_err());
    assert_eq!(encoded.as_slice(), b"abcdefg");
    assert_eq!(encoded.written, 7);
    assert_eq!(encoded.exceeded_at(), Some(15));
    assert_eq!(encoded.bytes.capacity(), 7);
    assert_eq!(*allocations.borrow(), [3, 6, 7]);
}

#[test]
fn allocation_refusal_precedes_initial_and_replacement_allocations() {
    let mut reject = |_| Err(io::Error::other("before allocation"));
    let mut encoded = BoundedEncodedBuffer::new(1024, true, Some(&mut reject));
    assert!(encoded.write_all(b"source").is_err());
    assert_eq!(encoded.bytes.capacity(), 0);
    assert!(encoded.as_slice().is_empty());
    assert_eq!(encoded.written, 0);
    assert_eq!(encoded.exceeded_at(), None);

    let mut first_allocation = true;
    let mut reject_growth = |_| {
        if std::mem::take(&mut first_allocation) {
            Ok(())
        } else {
            Err(io::Error::other("before replacement allocation"))
        }
    };
    let mut encoded = BoundedEncodedBuffer::new(1024, true, Some(&mut reject_growth));
    encoded.write_all(b"abc").unwrap();
    assert!(encoded.write_all(b"de").is_err());
    assert_eq!(encoded.bytes.capacity(), 3);
    assert_eq!(encoded.as_slice(), b"abc");
    assert_eq!(encoded.written, 3);
    assert_eq!(encoded.exceeded_at(), None);
}

#[test]
fn explicit_encoding_limits_keep_domain_admission_before_output() {
    let source = source();
    let limits = SceneFileLimits::default()
        .with_entities(ItemLimit::new(1).unwrap())
        .with_encoded_bytes(ByteLimit::new(1).unwrap());
    for result in [
        source.to_json_string_with_limits(limits),
        source.to_ron_string_with_limits(limits),
    ] {
        assert!(matches!(
            result,
            Err(SceneFormatError::Budget(SceneFileBudgetError {
                kind: SceneFileBudgetKind::Entities,
                observed: 2,
                maximum: 1,
            }))
        ));
    }
}

fn assert_encoding_boundary<T: SceneFilePayload>(
    payload: &T,
    encoding: SceneFileEncoding,
    encode: fn(&T, SceneFileLimits) -> Result<String, SceneFormatError>,
) {
    let expected = reference(payload, encoding);
    let exact =
        SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(expected.len()).unwrap());
    assert_eq!(encode(payload, exact).unwrap(), expected);
    let short = exact.with_encoded_bytes(ByteLimit::new(expected.len() - 1).unwrap());
    assert!(matches!(
        encode(payload, short),
        Err(SceneFormatError::EncodedBytesExceeded { observed, maximum })
            if observed == expected.len() && maximum == expected.len() - 1
    ));
}

// Retain the original encoders as the output oracle, including their envelope/configuration.
fn reference<T: SceneFilePayload>(payload: &T, encoding: SceneFileEncoding) -> String {
    let envelope = canonical_envelope(payload, SceneFileLimits::default()).unwrap();
    match encoding {
        SceneFileEncoding::Json => serde_json::to_string_pretty(&envelope).unwrap(),
        SceneFileEncoding::Ron => {
            ron::ser::to_string_pretty(&envelope, canonical_ron_pretty_config()).unwrap()
        }
    }
}

fn source() -> SceneDocument {
    let record = SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "escaped",
                ComponentValue::String(
                    "quotes\" slash\\ newline\n tab\t NUL\0 中文🦀\u{2028}".repeat(2048),
                ),
            ),
            (
                "values",
                ComponentValue::List(vec![
                    ComponentValue::Null,
                    ComponentValue::Bool(true),
                    ComponentValue::I64(i64::MIN),
                    ComponentValue::U64(u64::MAX),
                ]),
            ),
        ]),
    )
    .with_contextual_default(
        ComponentFieldId::new("fallback"),
        ComponentValue::String("preserved source default".to_owned()),
    );
    let mut source = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("z").unwrap())
            .with_component(ComponentTypeId::new("test.Value"), record),
        SceneEntityRecord::new(SceneEntityId::new("a").unwrap()),
    ]);
    source.entities.reverse();
    source
}

fn prefab(source: &SceneDocument) -> PrefabDocument {
    PrefabDocument {
        inputs: BTreeSet::from([ReferenceInputId::new("destination").unwrap()]),
        placements: source.placements.clone(),
        entities: source.entities.clone(),
    }
}
