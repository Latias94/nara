//! Closed source encoding cannot replace an open document's actual saved checkpoint.

use super::*;

#[test]
fn closed_encoding_rejects_open_members_and_keeps_all_saved_captures_available() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut prepared = stage(&workspace, &basis, documents);
    for index in [0, 1, 2, 3, 4, usize::MAX] {
        let before = prepared.usage();
        assert!(matches!(
            prepared.encode_closed_source_output(
                PrefabExtractionSourceOutput::Existing(index),
                SceneFileEncoding::Json,
                SceneFileLimits::default(),
            ),
            Err(SceneFileEncodingAdmissionError::Admission(_))
        ));
        assert!(prepared.usage().work_bytes > before.work_bytes);
    }
    for entry in &prepared.entries {
        if let Some(save) = &entry.save {
            assert!(matches!(save.evidence, SavedEvidence::Available(_)));
        }
    }
    let encoded = prepared
        .encode_checkpoint(documents[0], SceneFileEncoding::Json)
        .unwrap();
    assert_eq!(encoded.checkpoint().document(), documents[0]);
    prepared
        .return_checkpoint(encoded.into_checkpoint())
        .unwrap();
    assert_eq!(snapshot(&workspace), before);
}

#[test]
fn extracted_output_has_its_original_work_and_encoding_refusal_preserves_saved_evidence() {
    let basis = basis();
    let (workspace, _, documents) = fixture(&basis);
    let before = snapshot(&workspace);
    let mut prepared = stage(&workspace, &basis, documents);
    let (other, _, other_documents) = fixture(&basis);
    let mut foreign = stage(&other, &basis, other_documents);
    for encoding in [SceneFileEncoding::Json, SceneFileEncoding::Ron] {
        let output = prepared
            .encode_closed_source_output(
                PrefabExtractionSourceOutput::Extracted,
                encoding,
                SceneFileLimits::default(),
            )
            .unwrap();
        prepared.require_encoded_source_output(&output).unwrap();
        let other = foreign
            .encode_closed_source_output(
                PrefabExtractionSourceOutput::Extracted,
                encoding,
                SceneFileLimits::default(),
            )
            .unwrap();
        assert_eq!(other.bytes(), output.bytes());
        assert!(prepared.require_encoded_source_output(&other).is_err());
        let spent = prepared.usage();
        assert!(matches!(
            prepared.encode_closed_source_output(
                PrefabExtractionSourceOutput::Extracted,
                encoding,
                SceneFileLimits::default()
                    .with_encoded_bytes(ByteLimit::new(output.bytes().len() - 1).unwrap()),
            ),
            Err(SceneFileEncodingAdmissionError::Format(_))
        ));
        assert!(prepared.usage().work_bytes > spent.work_bytes);
        for entry in &prepared.entries {
            if let Some(save) = &entry.save {
                assert!(matches!(save.evidence, SavedEvidence::Available(_)));
            }
        }
    }
    assert_eq!(snapshot(&workspace), before);
}
