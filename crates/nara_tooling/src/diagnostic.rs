use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticSeverity,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_reflect::ComponentTypeId;
use nara_scene::SceneEntityId;

pub(crate) fn error(code: &'static str, summary: &'static str) -> Diagnostic {
    diagnostic(code, DiagnosticSeverity::Error, summary)
}

pub(crate) fn warning(code: &'static str, summary: &'static str) -> Diagnostic {
    diagnostic(code, DiagnosticSeverity::Warning, summary)
}

fn diagnostic(
    code: &'static str,
    severity: DiagnosticSeverity,
    summary: &'static str,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::new(code).expect("tooling diagnostic code literals must be valid"),
        severity,
        SafeSummary::new(summary).expect("tooling diagnostic summaries must be safe literals"),
    )
}

pub(crate) fn with_entity(diagnostic: Diagnostic, entity: &SceneEntityId) -> Diagnostic {
    with_public_identifier(diagnostic, "entity", entity.as_str())
}

pub(crate) fn with_component(diagnostic: Diagnostic, component: &ComponentTypeId) -> Diagnostic {
    with_public_identifier(diagnostic, "component", component.as_str())
}

pub(crate) fn with_public_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    let key = field_key(key);
    let field = PublicDiagnosticIdentifier::new(value)
        .map(|value| DiagnosticField::public_identifier(key, value))
        .unwrap_or_else(|_| DiagnosticField::sensitive(key));
    with_field(diagnostic, field)
}

pub(crate) fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    with_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

pub(crate) fn with_sensitive(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    with_field(diagnostic, DiagnosticField::sensitive(field_key(key)))
}

pub(crate) fn with_secret(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    with_field(diagnostic, DiagnosticField::secret(field_key(key)))
}

fn with_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("tooling diagnostics must use unique fields within the hard field limit")
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("tooling diagnostic field key literals must be valid")
}

#[cfg(test)]
mod tests {
    use nara_diagnostic::{DiagnosticFieldClass, DiagnosticValueRef};

    use super::*;

    #[test]
    fn location_fields_are_typed_and_untrusted_component_ids_fail_closed() {
        let entity = SceneEntityId::new("enemy/visual").unwrap();
        let component = ComponentTypeId::new("api_key-live-value");
        let entry = with_public_u64(
            with_component(
                with_entity(error("tooling.test", "tooling test failed"), &entity),
                &component,
            ),
            "operation_count",
            3,
        );

        assert!(matches!(
            entry.fields()[0].value(),
            DiagnosticValueRef::Identifier("enemy/visual")
        ));
        assert_eq!(entry.fields()[1].class(), DiagnosticFieldClass::Sensitive);
        assert_eq!(entry.fields()[1].value(), DiagnosticValueRef::Redacted);
        assert_eq!(entry.fields()[2].value(), DiagnosticValueRef::Unsigned(3));
        assert!(!format!("{entry:?}").contains(component.as_str()));
    }

    #[test]
    fn valid_but_sensitive_or_oversized_entity_ids_fail_closed() {
        let sensitive = SceneEntityId::new("api_key").unwrap();
        let oversized_raw = "a".repeat(97);
        let oversized = SceneEntityId::new(oversized_raw.clone()).unwrap();

        for entity in [&sensitive, &oversized] {
            let entry = with_entity(error("tooling.test", "tooling test failed"), entity);
            assert_eq!(entry.fields()[0].class(), DiagnosticFieldClass::Sensitive);
            assert_eq!(entry.fields()[0].value(), DiagnosticValueRef::Redacted);
            assert!(!format!("{entry:?}").contains(entity.as_str()));
        }
    }
}
