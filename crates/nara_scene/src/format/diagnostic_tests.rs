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
        payload: (placements: [], entities: []),
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
        assert!(error.parser_location().is_some(), "{error:?}");
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

#[test]
fn malformed_nested_component_values_keep_payload_stage_and_real_parser_locations() {
    let document = crate::SceneDocument::from_ordered_roots([crate::SceneEntityRecord::new(
        crate::SceneEntityId::new("root").unwrap(),
    )
    .with_component(
        nara_reflect::ComponentTypeId::new("test.Value"),
        crate::SceneComponentRecord::new(
            nara_reflect::ComponentSchemaVersion::ONE,
            nara_reflect::ComponentValue::List(vec![nara_reflect::ComponentValue::Null]),
        ),
    )]);
    let json = document.to_json_string().unwrap();
    let ron = document.to_ron_string().unwrap();
    for (wire, encoding) in [
        (json, super::SceneFileEncoding::Json),
        (ron, super::SceneFileEncoding::Ron),
    ] {
        assert_eq!(wire.matches("\"null\"").count(), 1);
        let malformed = wire.replace("\"null\"", "\"bool\"");
        let error = match encoding {
            super::SceneFileEncoding::Json => {
                SceneDocumentCandidate::decode_json_str(&malformed).unwrap_err()
            }
            super::SceneFileEncoding::Ron => {
                SceneDocumentCandidate::decode_ron_str(&malformed).unwrap_err()
            }
        };
        assert!(
            matches!(error, SceneFormatError::Payload { .. }),
            "{error:?}"
        );
        assert!(!error.is_syntax_error(), "{error:?}");
        assert!(error.shape_error().is_none());
        assert!(matches!(error.parser_location(), Some((line, column)) if line > 0 && column > 0));
    }
}
