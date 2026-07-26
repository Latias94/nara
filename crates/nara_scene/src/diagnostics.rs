use nara_asset::AssetRef;
use nara_diagnostic::{
    Diagnostic, DiagnosticCode, DiagnosticField, DiagnosticFieldKey, DiagnosticReport,
    PublicDiagnosticIdentifier, SafeSummary,
};
use nara_identity::{
    __private::IdentitySupportTopologyError, EntityIdentityAxis, IdentityDomainError,
};
use nara_reflect::{
    ComponentCapability, ComponentCodecError, ComponentEntityReferenceRewriteError,
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
    ComponentMigrationError, ComponentValueKind, PersistentApplyRejection,
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

pub(crate) fn with_entity_reference_rewrite_error<E>(
    diagnostic: Diagnostic,
    error: &ComponentEntityReferenceRewriteError<E>,
    rewrite_error_kind: impl FnOnce(&E) -> &'static str,
) -> Diagnostic {
    match error {
        ComponentEntityReferenceRewriteError::NodeLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "node-limit"),
            "maximum-nodes",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::ByteLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "byte-limit"),
            "maximum-bytes",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::DepthLimit { maximum } => with_public_u64(
            with_public_identifier(diagnostic, "rewrite-error-kind", "depth-limit"),
            "maximum-depth",
            usize_to_u64(*maximum),
        ),
        ComponentEntityReferenceRewriteError::PathIndexOverflow => {
            with_public_identifier(diagnostic, "rewrite-error-kind", "path-index-overflow")
        }
        ComponentEntityReferenceRewriteError::DuplicateDeclaredPath { path } => {
            with_reference_rewrite_path(
                with_public_identifier(diagnostic, "rewrite-error-kind", "duplicate-declared-path"),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::UndeclaredReference { path } => {
            with_reference_rewrite_path(
                with_public_identifier(diagnostic, "rewrite-error-kind", "undeclared-reference"),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::MissingEntityRefCapability { path } => {
            with_reference_rewrite_path(
                with_public_identifier(
                    diagnostic,
                    "rewrite-error-kind",
                    "missing-entity-ref-capability",
                ),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::RequiredReferenceMissing { path } => {
            with_reference_rewrite_path(
                with_public_identifier(
                    diagnostic,
                    "rewrite-error-kind",
                    "required-reference-missing",
                ),
                path,
            )
        }
        ComponentEntityReferenceRewriteError::InvalidReferenceValue { path, actual } => {
            with_public_identifier(
                with_reference_rewrite_path(
                    with_public_identifier(
                        diagnostic,
                        "rewrite-error-kind",
                        "invalid-reference-value",
                    ),
                    path,
                ),
                "actual-value-kind",
                component_value_kind_name(*actual),
            )
        }
        ComponentEntityReferenceRewriteError::InvalidPath { path, error } => {
            with_component_field_path_error(
                with_reference_rewrite_path(
                    with_public_identifier(diagnostic, "rewrite-error-kind", "invalid-path"),
                    path,
                ),
                error,
            )
        }
        ComponentEntityReferenceRewriteError::Rewrite { path, error } => {
            with_reference_rewrite_path(
                with_public_identifier(diagnostic, "rewrite-error-kind", rewrite_error_kind(error)),
                path,
            )
        }
    }
}

fn with_reference_rewrite_path(diagnostic: Diagnostic, path: &ComponentFieldPath) -> Diagnostic {
    with_component_field_path(
        diagnostic,
        "reference-field-path",
        "reference-field-path-depth",
        path,
    )
}

const fn component_value_kind_name(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Null => "null",
        ComponentValueKind::Bool => "bool",
        ComponentValueKind::I64 => "i64",
        ComponentValueKind::U64 => "u64",
        ComponentValueKind::F64 => "f64",
        ComponentValueKind::String => "string",
        ComponentValueKind::List => "list",
        ComponentValueKind::Map => "map",
        ComponentValueKind::AssetRef => "asset-ref",
        ComponentValueKind::EntityRef => "entity-ref",
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
        ComponentCodecError::WrongWorld => {
            with_public_identifier(diagnostic, "codec-error-kind", "wrong-world")
        }
        ComponentCodecError::AssetServerChanged => {
            with_public_identifier(diagnostic, "codec-error-kind", "asset-server-changed")
        }
        ComponentCodecError::PreparedComponentTypeMismatch { .. } => with_secret(
            with_public_identifier(
                diagnostic,
                "codec-error-kind",
                "prepared-component-type-mismatch",
            ),
            "codec-detail",
        ),
        ComponentCodecError::PersistentApplyReceiptMissing => with_public_identifier(
            diagnostic,
            "codec-error-kind",
            "persistent-apply-receipt-missing",
        ),
        ComponentCodecError::PersistentApplyTargetNotEmpty => with_public_identifier(
            diagnostic,
            "codec-error-kind",
            "persistent-apply-target-not-empty",
        ),
        ComponentCodecError::PersistentApplyBindingConflict { component_id } => {
            with_public_locator(
                with_public_identifier(
                    diagnostic,
                    "codec-error-kind",
                    "persistent-apply-binding-conflict",
                ),
                "persistent-component-id",
                component_id.as_str(),
            )
        }
        ComponentCodecError::PersistentApplySupportRejected { reason } => {
            with_persistent_apply_rejection(
                with_public_identifier(
                    diagnostic,
                    "codec-error-kind",
                    "persistent-apply-support-rejected",
                ),
                reason,
            )
        }
        ComponentCodecError::PersistentApplyRejected {
            component_id,
            reason,
        } => {
            let diagnostic = with_public_locator(
                with_public_identifier(diagnostic, "codec-error-kind", "persistent-apply-rejected"),
                "persistent-component-id",
                component_id.as_str(),
            );
            with_persistent_apply_rejection(diagnostic, reason)
        }
        ComponentCodecError::Message(_) => with_secret(
            with_public_identifier(diagnostic, "codec-error-kind", "message"),
            "codec-detail",
        ),
    }
}

fn with_persistent_apply_rejection(
    diagnostic: Diagnostic,
    reason: &PersistentApplyRejection,
) -> Diagnostic {
    match reason {
        PersistentApplyRejection::ComponentMetadataMissing => with_public_identifier(
            diagnostic,
            "persistent-apply-reason",
            "component-metadata-missing",
        ),
        PersistentApplyRejection::RequiredComponents => {
            with_public_identifier(diagnostic, "persistent-apply-reason", "required-components")
        }
        PersistentApplyRejection::LifecycleHook { event } => with_public_identifier(
            with_public_identifier(diagnostic, "persistent-apply-reason", "lifecycle-hook"),
            "lifecycle-event",
            event.as_str(),
        ),
        PersistentApplyRejection::Observer { event, scope } => with_public_identifier(
            with_public_identifier(
                with_public_identifier(diagnostic, "persistent-apply-reason", "lifecycle-observer"),
                "lifecycle-event",
                event.as_str(),
            ),
            "observer-scope",
            scope.as_str(),
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

pub(crate) fn with_identity_error(
    diagnostic: Diagnostic,
    error: &IdentityDomainError,
) -> Diagnostic {
    match error {
        IdentityDomainError::WorldDomainIdExhausted => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "world-domain-id-exhausted",
        ),
        IdentityDomainError::WorldDomainUnavailable => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "world-domain-unavailable",
        ),
        IdentityDomainError::WorldBindingMismatch => {
            with_public_identifier(diagnostic, "identity-error-kind", "world-binding-mismatch")
        }
        IdentityDomainError::SceneInstanceExhausted => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "scene-instance-exhausted",
        ),
        IdentityDomainError::SceneInstanceAlreadyClaimed { instance } => with_public_u64(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "scene-instance-already-claimed",
            ),
            "scene-instance",
            instance.get(),
        ),
        IdentityDomainError::SceneInstanceNotActive { instance } => with_public_u64(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "scene-instance-not-active",
            ),
            "scene-instance",
            instance.get(),
        ),
        IdentityDomainError::SceneInstanceMembershipMismatch { instance } => with_public_u64(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "scene-instance-membership-mismatch",
            ),
            "scene-instance",
            instance.get(),
        ),
        IdentityDomainError::ReferenceAlreadyClaimed { .. } => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "reference-already-claimed",
        ),
        IdentityDomainError::EntityAxisAlreadyRegistered { axis } => with_public_identifier(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "entity-axis-already-registered",
            ),
            "identity-axis",
            match axis {
                EntityIdentityAxis::Scene => "scene",
                EntityIdentityAxis::Persistent => "persistent",
            },
        ),
        IdentityDomainError::EntityTokenNotAlive => {
            with_public_identifier(diagnostic, "identity-error-kind", "entity-token-not-alive")
        }
        IdentityDomainError::EntityTokenNotOwned => {
            with_public_identifier(diagnostic, "identity-error-kind", "entity-token-not-owned")
        }
        IdentityDomainError::DuplicateSceneEntityId { entity } => with_public_locator(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "duplicate-scene-entity-id",
            ),
            "entity-id",
            entity.as_str(),
        ),
        IdentityDomainError::DuplicateRuntimeEntity => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "duplicate-runtime-entity",
        ),
        IdentityDomainError::IncompleteSceneFork => {
            with_public_identifier(diagnostic, "identity-error-kind", "incomplete-scene-fork")
        }
        IdentityDomainError::IncompleteSceneForkIdentityAxes { entity } => with_public_locator(
            with_public_identifier(
                diagnostic,
                "identity-error-kind",
                "incomplete-scene-fork-identity-axes",
            ),
            "entity-id",
            entity.as_str(),
        ),
        IdentityDomainError::InvalidSceneRemap(_) => {
            with_public_identifier(diagnostic, "identity-error-kind", "invalid-scene-remap")
        }
        IdentityDomainError::LifetimeClaimLimit { requested, maximum } => with_public_u64(
            with_public_u64(
                with_public_identifier(diagnostic, "identity-error-kind", "lifetime-claim-limit"),
                "requested-claims",
                usize_to_u64(*requested),
            ),
            "maximum-claims",
            usize_to_u64(*maximum),
        ),
        IdentityDomainError::WrongDomain { expected, actual } => with_public_u64(
            with_public_u64(
                with_public_identifier(diagnostic, "identity-error-kind", "wrong-domain"),
                "expected-domain",
                expected.get(),
            ),
            "actual-domain",
            actual.get(),
        ),
        IdentityDomainError::EntityNotRegistered => {
            with_public_identifier(diagnostic, "identity-error-kind", "entity-not-registered")
        }
        IdentityDomainError::StaleRegistration => {
            with_public_identifier(diagnostic, "identity-error-kind", "stale-registration")
        }
        IdentityDomainError::RetirementSequenceExhausted => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "retirement-sequence-exhausted",
        ),
        IdentityDomainError::LifecycleConflict => {
            with_public_identifier(diagnostic, "identity-error-kind", "lifecycle-conflict")
        }
    }
}

pub(crate) fn with_identity_support_error(
    diagnostic: Diagnostic,
    error: &IdentitySupportTopologyError,
) -> Diagnostic {
    with_public_identifier(
        diagnostic,
        "identity-support-error-kind",
        match error {
            IdentitySupportTopologyError::LifecycleConflict => "lifecycle-conflict",
            IdentitySupportTopologyError::TargetMissing => "target-missing",
        },
    )
}

pub(crate) fn with_capability(
    diagnostic: Diagnostic,
    capability: ComponentCapability,
) -> Diagnostic {
    let capability = match capability {
        ComponentCapability::Scene => "scene",
        ComponentCapability::Inspect => "inspect",
        ComponentCapability::Edit => "edit",
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
