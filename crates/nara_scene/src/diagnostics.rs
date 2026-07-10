use nara_asset::AssetRef;
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentFieldPath, ComponentFieldPathError,
    ComponentFieldPathSegment, ComponentMigrationError,
};

const MAX_SCENE_FIELD_PATH_LOCATOR_BYTES: usize = 96;

pub(crate) fn error(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::error(diagnostic_code(code), safe_summary(summary))
}

pub(crate) fn warning(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::warning(diagnostic_code(code), safe_summary(summary))
}

pub(crate) fn info(code: &'static str, summary: &'static str) -> Diagnostic {
    Diagnostic::info(diagnostic_code(code), safe_summary(summary))
}

pub(crate) fn with_public_locator(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &str,
) -> Diagnostic {
    let key = field_key(key);
    let field = match PublicDiagnosticIdentifier::new(value) {
        Ok(value) => DiagnosticField::public_identifier(key, value),
        Err(_) => DiagnosticField::sensitive(key),
    };
    with_field(diagnostic, field)
}

pub(crate) fn with_public_identifier(
    diagnostic: Diagnostic,
    key: &'static str,
    value: &'static str,
) -> Diagnostic {
    let value = PublicDiagnosticIdentifier::new(value)
        .expect("scene diagnostic identifier literals must be valid");
    with_field(
        diagnostic,
        DiagnosticField::public_identifier(field_key(key), value),
    )
}

pub(crate) fn with_public_u64(diagnostic: Diagnostic, key: &'static str, value: u64) -> Diagnostic {
    with_field(
        diagnostic,
        DiagnosticField::public_u64(field_key(key), value),
    )
}

pub(crate) fn push_with_operation_index(
    report: &mut DiagnosticReport,
    diagnostic: Diagnostic,
    operation_index: Option<usize>,
) {
    let diagnostic = match operation_index {
        Some(operation_index) => {
            with_public_u64(diagnostic, "operation-index", usize_to_u64(operation_index))
        }
        None => diagnostic,
    };
    report.push(diagnostic);
}

pub(crate) fn with_component_field_path(
    diagnostic: Diagnostic,
    key: &'static str,
    depth_key: &'static str,
    path: &ComponentFieldPath,
) -> Diagnostic {
    let diagnostic = with_public_u64(diagnostic, depth_key, usize_to_u64(path.segments().len()));
    let key = field_key(key);
    let field = match lower_component_field_path(path) {
        Some(value) => DiagnosticField::public_identifier(key, value),
        None => DiagnosticField::sensitive(key),
    };
    with_field(diagnostic, field)
}

pub(crate) fn with_component_field_path_error(
    diagnostic: Diagnostic,
    error: &ComponentFieldPathError,
) -> Diagnostic {
    match error {
        ComponentFieldPathError::EmptyPath => {
            with_public_identifier(diagnostic, "path-error-kind", "empty-path")
        }
        ComponentFieldPathError::ExpectedMap { path } => with_component_field_path(
            with_public_identifier(diagnostic, "path-error-kind", "expected-map"),
            "error-field-path",
            "error-field-path-depth",
            path,
        ),
        ComponentFieldPathError::ExpectedList { path } => with_component_field_path(
            with_public_identifier(diagnostic, "path-error-kind", "expected-list"),
            "error-field-path",
            "error-field-path-depth",
            path,
        ),
        ComponentFieldPathError::MissingField { path, field } => with_public_field_segment(
            with_component_field_path(
                with_public_identifier(diagnostic, "path-error-kind", "missing-field"),
                "error-field-path",
                "error-field-path-depth",
                path,
            ),
            "missing-field",
            field,
        ),
        ComponentFieldPathError::IndexOutOfBounds { path, index, len } => with_public_u64(
            with_public_u64(
                with_component_field_path(
                    with_public_identifier(diagnostic, "path-error-kind", "index-out-of-bounds"),
                    "error-field-path",
                    "error-field-path-depth",
                    path,
                ),
                "path-error-index",
                u64::from(*index),
            ),
            "path-error-length",
            usize_to_u64(*len),
        ),
    }
}

pub(crate) fn with_sensitive(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    with_field(diagnostic, DiagnosticField::sensitive(field_key(key)))
}

pub(crate) fn with_secret(diagnostic: Diagnostic, key: &'static str) -> Diagnostic {
    with_field(diagnostic, DiagnosticField::secret(field_key(key)))
}

pub(crate) fn with_asset_ref(
    diagnostic: Diagnostic,
    key: &'static str,
    asset_ref: &AssetRef,
) -> Diagnostic {
    match asset_ref {
        AssetRef::Path(path) => {
            match DiagnosticField::project_relative(field_key(key), path.as_str()) {
                Ok(field) => with_field(diagnostic, field),
                Err(_) => with_sensitive(diagnostic, key),
            }
        }
        AssetRef::StableId(id) => {
            let stable_id = id.to_string();
            match PublicDiagnosticIdentifier::new(&stable_id) {
                Ok(value) => with_field(
                    diagnostic,
                    DiagnosticField::public_identifier(field_key(key), value),
                ),
                Err(_) => with_sensitive(diagnostic, key),
            }
        }
    }
}

pub(crate) fn with_codec_error(diagnostic: Diagnostic, error: &ComponentCodecError) -> Diagnostic {
    match error {
        ComponentCodecError::MissingField { field } => with_public_locator(
            with_public_identifier(diagnostic, "codec-error-kind", "missing-field"),
            "field-path",
            field,
        ),
        ComponentCodecError::InvalidField { field, .. } => with_secret(
            with_public_locator(
                with_public_identifier(diagnostic, "codec-error-kind", "invalid-field"),
                "field-path",
                field,
            ),
            "codec-detail",
        ),
        ComponentCodecError::InvalidAssetRef { field, .. } => with_secret(
            with_sensitive(
                with_public_locator(
                    with_public_identifier(diagnostic, "codec-error-kind", "invalid-asset-ref"),
                    "field-path",
                    field,
                ),
                "asset-ref",
            ),
            "codec-detail",
        ),
        ComponentCodecError::EntityMissing => {
            with_public_identifier(diagnostic, "codec-error-kind", "entity-missing")
        }
        ComponentCodecError::Message(_) => with_secret(
            with_public_identifier(diagnostic, "codec-error-kind", "message"),
            "codec-detail",
        ),
    }
}

pub(crate) fn with_migration_error(
    diagnostic: Diagnostic,
    error: &ComponentMigrationError,
) -> Diagnostic {
    match error {
        ComponentMigrationError::UnknownComponentId { .. } => {
            with_public_identifier(diagnostic, "migration-error-kind", "unknown-component")
        }
        ComponentMigrationError::UnsupportedVersion {
            from_version,
            target_version,
            ..
        } => with_public_u64(
            with_public_u64(
                with_public_identifier(diagnostic, "migration-error-kind", "unsupported-version"),
                "source-version",
                u64::from(from_version.0),
            ),
            "target-version",
            u64::from(target_version.0),
        ),
        ComponentMigrationError::MissingMigration {
            from_version,
            target_version,
            ..
        } => with_public_u64(
            with_public_u64(
                with_public_identifier(diagnostic, "migration-error-kind", "missing-migration"),
                "source-version",
                u64::from(from_version.0),
            ),
            "target-version",
            u64::from(target_version.0),
        ),
        ComponentMigrationError::MigrationFailed {
            from_version,
            to_version,
            error,
            ..
        } => with_codec_error(
            with_public_u64(
                with_public_u64(
                    with_public_identifier(diagnostic, "migration-error-kind", "migration-failed"),
                    "source-version",
                    u64::from(from_version.0),
                ),
                "target-version",
                u64::from(to_version.0),
            ),
            error,
        ),
    }
}

pub(crate) fn with_capability(
    diagnostic: Diagnostic,
    capability: ComponentCapability,
) -> Diagnostic {
    let capability = match capability {
        ComponentCapability::Scene => "scene",
        ComponentCapability::Inspect => "inspect",
        ComponentCapability::Edit => "edit",
        ComponentCapability::Animate => "animate",
        ComponentCapability::Replicate => "replicate",
        ComponentCapability::Script => "script",
        ComponentCapability::AssetRef => "asset-ref",
        ComponentCapability::EntityRef => "entity-ref",
    };
    with_public_identifier(diagnostic, "required-capability", capability)
}

pub(crate) fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn with_public_field_segment(diagnostic: Diagnostic, key: &'static str, value: &str) -> Diagnostic {
    let key = field_key(key);
    let field = match validate_public_field_segment(value) {
        Some(value) => DiagnosticField::public_identifier(key, value),
        None => DiagnosticField::sensitive(key),
    };
    with_field(diagnostic, field)
}

fn lower_component_field_path(path: &ComponentFieldPath) -> Option<PublicDiagnosticIdentifier> {
    if path.is_empty() {
        return PublicDiagnosticIdentifier::new("root").ok();
    }

    let mut lowered = String::with_capacity(MAX_SCENE_FIELD_PATH_LOCATOR_BYTES);
    for segment in path.segments() {
        if !lowered.is_empty() {
            push_bounded(&mut lowered, "/")?;
        }
        match segment {
            ComponentFieldPathSegment::Field(field) => {
                validate_public_field_segment(field)?;
                push_bounded(&mut lowered, "f_")?;
                push_bounded(&mut lowered, field)?;
            }
            ComponentFieldPathSegment::Index(index) => {
                push_bounded(&mut lowered, "i_")?;
                push_bounded(&mut lowered, &index.to_string())?;
            }
        }
    }
    PublicDiagnosticIdentifier::new(&lowered).ok()
}

fn validate_public_field_segment(value: &str) -> Option<PublicDiagnosticIdentifier> {
    if value.is_empty()
        || value.len() > MAX_SCENE_FIELD_PATH_LOCATOR_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return None;
    }
    PublicDiagnosticIdentifier::new(value).ok()
}

fn push_bounded(target: &mut String, value: &str) -> Option<()> {
    let next_len = target.len().checked_add(value.len())?;
    if next_len > MAX_SCENE_FIELD_PATH_LOCATOR_BYTES {
        return None;
    }
    target.push_str(value);
    Some(())
}

fn diagnostic_code(value: &'static str) -> DiagnosticCode {
    DiagnosticCode::new(value).expect("scene diagnostic code literals must be valid")
}

fn safe_summary(value: &'static str) -> SafeSummary {
    SafeSummary::new(value).expect("scene diagnostic summaries must be safe static text")
}

fn field_key(value: &'static str) -> DiagnosticFieldKey {
    DiagnosticFieldKey::new(value).expect("scene diagnostic field key literals must be valid")
}

fn with_field(diagnostic: Diagnostic, field: DiagnosticField) -> Diagnostic {
    diagnostic
        .try_with_field(field)
        .expect("scene diagnostics use unique fields below the hard field limit")
}
