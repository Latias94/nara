//! Safe format facts shared by strict content Check and ordinary authoring Open.

use nara_core::{EngineVersion, PersistentFileContractError, SerdeShapeError};
use nara_diagnostic::{Diagnostic, DiagnosticField, PublicDiagnosticIdentifier};
use nara_scene::{SceneFileBudgetKind, SceneFileEncoding, SceneFormatError};

use super::{attach_field, diagnostic, field_key};

/// Projects typed facts only. Parser messages can contain arbitrary document contents.
pub(crate) fn scene_format_diagnostic(
    code: &'static str,
    summary: &'static str,
    error: &SceneFormatError,
) -> Diagnostic {
    let diagnostic = diagnostic(code, summary);
    match error {
        SceneFormatError::EncodedBytesExceeded { observed, maximum } => {
            budget(diagnostic, "encoded-bytes", Some(*observed), *maximum)
        }
        SceneFormatError::Shape { encoding, .. }
        | SceneFormatError::Header { encoding, .. }
        | SceneFormatError::Payload { encoding, .. } => {
            let stage = match error {
                SceneFormatError::Shape { .. } => "shape-invalid",
                SceneFormatError::Header { .. } => "header-invalid",
                _ => "payload-invalid",
            };
            let diagnostic = identifier(
                diagnostic,
                "encoding",
                match encoding {
                    SceneFileEncoding::Json => "json",
                    SceneFileEncoding::Ron => "ron",
                },
            );
            parser_failure(diagnostic, error, stage)
        }
        SceneFormatError::Contract(PersistentFileContractError::UnsupportedFormatVersion {
            supported,
            actual,
            ..
        }) => {
            let diagnostic = identifier(diagnostic, "reason", "format-version-unsupported");
            let diagnostic = number(diagnostic, "observed_version", u64::from(actual.get()));
            number(diagnostic, "supported_version", u64::from(supported.get()))
        }
        SceneFormatError::Contract(PersistentFileContractError::WrongKind { expected, actual }) => {
            let diagnostic = identifier(diagnostic, "reason", "kind-mismatch");
            let diagnostic = format_kind(diagnostic, "expected_kind", expected.as_str());
            format_kind(diagnostic, "observed_kind", actual.as_str())
        }
        SceneFormatError::Contract(PersistentFileContractError::EngineVersionTooOld {
            current,
            required,
        }) => {
            let diagnostic = identifier(diagnostic, "reason", "engine-version-too-old");
            let diagnostic = engine_version(diagnostic, "current_engine", current);
            engine_version(diagnostic, "required_engine", required)
        }
        SceneFormatError::PatchFormatVersion { observed, expected } => {
            let diagnostic = identifier(diagnostic, "reason", "embedded-patch-version-unsupported");
            let diagnostic = number(diagnostic, "observed_version", u64::from(*observed));
            number(diagnostic, "supported_version", u64::from(*expected))
        }
        SceneFormatError::LegacyPlacementShape { entity_index } => {
            let diagnostic = identifier(diagnostic, "reason", "legacy-placement-shape");
            if let Some(index) = entity_index {
                number(diagnostic, "entity_index", usize_to_u64(*index))
            } else {
                diagnostic
            }
        }
        SceneFormatError::Budget(error) => budget(
            diagnostic,
            match error.kind {
                SceneFileBudgetKind::Entities => "entities",
                SceneFileBudgetKind::Placements => "placements",
                SceneFileBudgetKind::PlacementDepth => "placement-depth",
                SceneFileBudgetKind::Components => "components",
                SceneFileBudgetKind::PrefabInstances => "prefab-instances",
                SceneFileBudgetKind::PatchOperations => "patch-operations",
                SceneFileBudgetKind::OverlayDeclarations => "overlay-declarations",
                SceneFileBudgetKind::DiagnosticSources => "diagnostic-sources",
                SceneFileBudgetKind::ComponentValueNodes => "component-value-nodes",
                SceneFileBudgetKind::ComponentValueBytes => "component-value-bytes",
            },
            Some(error.observed),
            error.maximum,
        ),
        SceneFormatError::Encode { .. } => identifier(diagnostic, "reason", "encoding-failed"),
        SceneFormatError::Metadata(_) => identifier(diagnostic, "reason", "metadata-invalid"),
    }
}

fn parser_failure(
    mut diagnostic: Diagnostic,
    error: &SceneFormatError,
    stage: &'static str,
) -> Diagnostic {
    if let Some(shape) = error.shape_error() {
        return match shape {
            SerdeShapeError::DepthExceeded { maximum } => {
                budget(diagnostic, "shape-depth", None, *maximum)
            }
            SerdeShapeError::NodeLimitExceeded { maximum } => {
                budget(diagnostic, "shape-nodes", None, *maximum)
            }
            SerdeShapeError::ContainerItemLimitExceeded { maximum } => {
                budget(diagnostic, "container-items", None, *maximum)
            }
            SerdeShapeError::StringLimitExceeded { maximum } => {
                budget(diagnostic, "string-bytes", None, *maximum)
            }
            SerdeShapeError::TotalStringLimitExceeded { maximum } => {
                budget(diagnostic, "total-string-bytes", None, *maximum)
            }
            SerdeShapeError::DuplicateMapKey => {
                identifier(diagnostic, "reason", "duplicate-map-key")
            }
        };
    }
    diagnostic = identifier(
        diagnostic,
        "reason",
        if error.is_syntax_error() {
            "syntax-invalid"
        } else {
            stage
        },
    );
    if let Some((line, column)) = error.parser_location() {
        diagnostic = number(diagnostic, "line", usize_to_u64(line));
        return number(diagnostic, "column", usize_to_u64(column));
    }
    diagnostic
}

fn budget(
    diagnostic: Diagnostic,
    kind: &'static str,
    observed: Option<usize>,
    maximum: usize,
) -> Diagnostic {
    let diagnostic = identifier(diagnostic, "reason", "budget-exceeded");
    let diagnostic = identifier(diagnostic, "budget", kind);
    let diagnostic = if let Some(observed) = observed {
        number(diagnostic, "observed", usize_to_u64(observed))
    } else {
        diagnostic
    };
    number(diagnostic, "maximum", usize_to_u64(maximum))
}

fn identifier(diagnostic: Diagnostic, key: &'static str, value: &'static str) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_identifier(
            field_key(key),
            PublicDiagnosticIdentifier::new(value).expect("format labels are engine-owned"),
        ),
    )
}

fn format_kind(diagnostic: Diagnostic, key: &'static str, value: &str) -> Diagnostic {
    // Identifier syntax does not authorize publishing arbitrary document strings.
    match value {
        "scene" => identifier(diagnostic, key, "scene"),
        "prefab" => identifier(diagnostic, key, "prefab"),
        "scene_patch" => identifier(diagnostic, key, "scene_patch"),
        _ => attach_field(diagnostic, DiagnosticField::sensitive(field_key(key))),
    }
}

fn engine_version(
    diagnostic: Diagnostic,
    key: &'static str,
    version: &EngineVersion,
) -> Diagnostic {
    let version = version.as_semver();
    // Prerelease and build identifiers can contain private source text; expose only numeric facts.
    let core = format!("{}.{}.{}", version.major, version.minor, version.patch);
    attach_field(
        diagnostic,
        DiagnosticField::public_identifier(
            field_key(key),
            PublicDiagnosticIdentifier::new(&core)
                .expect("numeric version cores are public identifiers"),
        ),
    )
}

fn number(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    attach_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use nara_core::{EngineVersion, FormatKind, PersistentFileContractError};
    use nara_diagnostic::{DiagnosticFieldClass, DiagnosticValueRef};
    use nara_scene::{SceneDocumentCandidate, SceneFormatError};

    use super::scene_format_diagnostic;

    #[test]
    fn kind_diagnostics_publish_only_known_scene_format_labels() {
        for kind in [
            "scene",
            "prefab",
            "scene_patch",
            "private_document_sentinel",
        ] {
            let error = SceneFormatError::Contract(PersistentFileContractError::WrongKind {
                expected: FormatKind::new("scene").unwrap(),
                actual: FormatKind::new(kind).unwrap(),
            });
            let diagnostic = scene_format_diagnostic(
                "project.content.scene-format-invalid",
                "Project scene format is invalid",
                &error,
            );
            let observed = diagnostic
                .fields()
                .iter()
                .find(|field| field.key().as_str() == "observed_kind")
                .unwrap();
            if kind == "private_document_sentinel" {
                assert_eq!(observed.class(), DiagnosticFieldClass::Sensitive);
                assert_eq!(observed.value(), DiagnosticValueRef::Redacted);
            } else {
                assert_eq!(observed.class(), DiagnosticFieldClass::Public);
                assert_eq!(observed.value(), DiagnosticValueRef::Identifier(kind));
            }
            assert!(!format!("{diagnostic:?}").contains("private_document_sentinel"));
        }
    }

    #[test]
    fn engine_diagnostics_publish_numeric_cores_for_both_versions() {
        let error = SceneFormatError::Contract(PersistentFileContractError::EngineVersionTooOld {
            current: EngineVersion::parse("0.1.0-private-current+privatecurrentbuild").unwrap(),
            required: EngineVersion::parse("999.2.3-private-required+privaterequiredbuild")
                .unwrap(),
        });
        let diagnostic = scene_format_diagnostic(
            "project.content.scene-format-invalid",
            "Project scene format is invalid",
            &error,
        );
        for (key, core) in [("current_engine", "0.1.0"), ("required_engine", "999.2.3")] {
            assert!(diagnostic.fields().iter().any(|field| {
                field.key().as_str() == key
                    && field.class() == DiagnosticFieldClass::Public
                    && field.value() == DiagnosticValueRef::Identifier(core)
            }));
        }
        assert!(!format!("{diagnostic:?}").contains("private"));
    }

    #[test]
    fn ron_format_diagnostic_retains_safe_location_without_document_text() {
        let error = SceneDocumentCandidate::decode_ron_bytes(
            br#"(kind: "scene", private_document_sentinel: "unterminated)"#,
        )
        .unwrap_err();
        let diagnostic = scene_format_diagnostic(
            "project.content.scene-format-invalid",
            "Project scene format is invalid",
            &error,
        );

        assert!(diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "reason"
                && field.value() == DiagnosticValueRef::Identifier("syntax-invalid")
        }));
        assert!(diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "line" && field.value() == DiagnosticValueRef::Unsigned(1)
        }));
        assert!(diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "column"
                && matches!(field.value(), DiagnosticValueRef::Unsigned(column) if column > 1)
        }));
        assert!(!format!("{diagnostic:?}").contains("private_document_sentinel"));
    }

    #[test]
    fn ron_data_failures_retain_header_and_payload_stages() {
        let encoded = r#"(
            kind: "scene",
            format_version: 1,
            engine_min_version: "0.1.0",
            generator: (name: "test", version: "0.1.0"),
            payload: (roots: [], entities: []),
        )"#
        .to_owned();
        for (source, reason) in [
            (
                encoded.replace("generator: (name: \"test\", version: \"0.1.0\"),", ""),
                "header-invalid",
            ),
            (
                encoded.replace("entities: []", "entities: true"),
                "payload-invalid",
            ),
        ] {
            let error = SceneDocumentCandidate::decode_ron_bytes(source.as_bytes()).unwrap_err();
            assert!(!error.is_syntax_error(), "{error:?}");
            let diagnostic = scene_format_diagnostic(
                "project.content.scene-format-invalid",
                "Project scene format is invalid",
                &error,
            );
            assert!(
                diagnostic.fields().iter().any(|field| {
                    field.key().as_str() == "reason"
                        && field.value() == DiagnosticValueRef::Identifier(reason)
                }),
                "{diagnostic:?}"
            );
        }
    }
}
