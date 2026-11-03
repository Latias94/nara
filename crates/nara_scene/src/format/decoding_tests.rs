use super::*;

fn source() -> SceneDocument {
    let mut source = SceneDocument::from_ordered_roots([
        SceneEntityRecord::new(SceneEntityId::new("z").unwrap()),
        SceneEntityRecord::new(SceneEntityId::new("a").unwrap()),
    ]);
    source.entities.reverse();
    source
}

fn wire<T: SceneFilePayload>(document: &T, encoding: SceneFileEncoding) -> Vec<u8> {
    // Preserve the deliberately unsorted wire order. The ordinary writer canonicalizes it.
    let envelope = publication_envelope(document, T::KIND).unwrap();
    match encoding {
        SceneFileEncoding::Json => serde_json::to_vec_pretty(&envelope).unwrap(),
        SceneFileEncoding::Ron => {
            ron::ser::to_string_pretty(&envelope, canonical_ron_pretty_config())
                .unwrap()
                .into_bytes()
        }
    }
}

#[test]
fn decoded_source_admission_precedes_domain_ordering_and_preserves_the_exact_refusal() {
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let source = source();
        let prefab = PrefabDocument {
            inputs: BTreeSet::new(),
            placements: source.placements.clone(),
            entities: source.entities.clone(),
        };
        let limits = SceneFileLimits::default().with_entities(ItemLimit::new(1).unwrap());
        let mut refusal = Some(Box::new(73_usize));
        let pointer = std::ptr::from_ref(refusal.as_deref().unwrap());
        let error = SceneDocumentCandidate::decode_bytes_with_admission(
            &wire(&source, encoding),
            encoding,
            limits,
            |raw| {
                assert_eq!(raw, &source);
                assert_eq!(raw.entities[0].id.as_str(), "z");
                Err(refusal.take().unwrap())
            },
        )
        .unwrap_err();
        let SceneFileDecodingAdmissionError::Admission(error) = error else {
            panic!("domain checks ran before the enclosing admission")
        };
        assert_eq!(std::ptr::from_ref(error.as_ref()), pointer);
        let error = PrefabDocumentCandidate::decode_bytes_with_admission(
            &wire(&prefab, encoding),
            encoding,
            limits,
            |raw| {
                assert_eq!(raw, &prefab);
                Err::<(), _>("prefab refusal")
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            SceneFileDecodingAdmissionError::Admission("prefab refusal")
        ));
    }
}

#[test]
fn admitted_decoders_keep_the_ordinary_codec_and_do_not_call_admission_for_rejected_wire() {
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let source = source();
        let prefab = PrefabDocument {
            inputs: BTreeSet::new(),
            placements: source.placements.clone(),
            entities: source.entities.clone(),
        };
        let bytes = wire(&source, encoding);
        let mut calls = 0;
        let candidate = SceneDocumentCandidate::decode_bytes_with_admission(
            &bytes,
            encoding,
            SceneFileLimits::default(),
            |raw| {
                calls += 1;
                assert_eq!(raw, &source);
                Ok::<(), ()>(())
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
        let document = candidate.into_composition_source().unwrap();
        assert_eq!(document.entities[0].id.as_str(), "a");
        let prefab_bytes = wire(&prefab, encoding);
        let candidate = PrefabDocumentCandidate::decode_bytes_with_admission(
            &prefab_bytes,
            encoding,
            SceneFileLimits::default(),
            |raw| {
                assert_eq!(raw, &prefab);
                Ok::<(), ()>(())
            },
        )
        .unwrap();
        assert_eq!(
            candidate.into_composition_source().unwrap().entities,
            document.entities
        );
        let short =
            SceneFileLimits::default().with_encoded_bytes(ByteLimit::new(bytes.len() - 1).unwrap());
        assert!(matches!(
            SceneDocumentCandidate::decode_bytes_with_admission(
                &bytes,
                encoding,
                short,
                |_| -> Result<(), ()> { panic!("wire limit must precede admission") }
            ),
            Err(SceneFileDecodingAdmissionError::Format(
                SceneFormatError::EncodedBytesExceeded { .. }
            ))
        ));
    }
}
