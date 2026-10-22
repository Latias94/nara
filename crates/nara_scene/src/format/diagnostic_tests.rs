use super::{SceneDocumentCandidate, SceneFormatError};

#[test]
fn ron_attribute_and_character_syntax_errors_retain_safe_locations() {
    for source in [b"#bogus".as_slice(), b"'ab'".as_slice()] {
        let error = SceneDocumentCandidate::decode_ron_bytes(source).unwrap_err();

        assert!(error.is_syntax_error(), "{error:?}");
        assert!(matches!(error.parser_location(), Some((1, column)) if column > 0));
        assert!(error.shape_error().is_none());
    }
}

#[test]
fn valid_ron_syntax_with_wrong_field_types_retains_the_decode_stage() {
    let source = r#"(
        kind: "scene",
        format_version: 1,
        engine_min_version: "0.1.0",
        generator: (name: "test", version: "0.1.0"),
        payload: (roots: [], entities: []),
    )"#;
    for (source, header) in [
        (
            source.replace("format_version: 1", "format_version: true"),
            true,
        ),
        (source.replace("entities: []", "entities: true"), false),
    ] {
        let error = SceneDocumentCandidate::decode_ron_bytes(source.as_bytes()).unwrap_err();

        assert!(!error.is_syntax_error(), "{error:?}");
        assert!(error.parser_location().is_some());
        assert!(error.shape_error().is_none());
        assert!(
            matches!(
                (&error, header),
                (SceneFormatError::Header { .. }, true) | (SceneFormatError::Payload { .. }, false)
            ),
            "{error:?}"
        );
    }
}
