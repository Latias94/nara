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
    ComponentFieldPath, ComponentFieldPathError, ComponentMigrationError, ComponentSchema,
    ComponentValueKind, PersistentApplyRejection,
};

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
    with_sensitive(
        with_public_u64(diagnostic, depth_key, usize_to_u64(path.segments().len())),
        key,
    )
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
        ComponentFieldPathError::MissingField { path, .. } => with_sensitive(
            with_component_field_path(
                with_public_identifier(diagnostic, "path-error-kind", "missing-field"),
                "error-field-path",
                "error-field-path-depth",
                path,
            ),
            "missing-field",
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
        ComponentEntityReferenceRewriteError::MissingAssetRefCapability { path } => {
            with_reference_rewrite_path(
                with_public_identifier(
                    diagnostic,
                    "rewrite-error-kind",
                    "missing-asset-ref-capability",
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

pub(crate) fn with_codec_error(
    diagnostic: Diagnostic,
    error: &ComponentCodecError,
    schema: Option<&ComponentSchema>,
) -> Diagnostic {
    match error {
        ComponentCodecError::UnknownComponent { .. } => {
            with_public_identifier(diagnostic, "codec-error-kind", "unknown-component")
        }
        ComponentCodecError::MissingNativeBinding { .. } => {
            with_public_identifier(diagnostic, "codec-error-kind", "missing-native-binding")
        }
        ComponentCodecError::Domain(error) => {
            let mut domain = Diagnostic::new(error.code(), diagnostic.severity(), error.summary());
            for field in diagnostic.fields() {
                domain = with_field(domain, field.clone());
            }
            domain =
                with_public_identifier(domain, "scene-context-code", diagnostic.code().as_str());
            domain = with_public_identifier(domain, "codec-error-kind", "domain");
            if let Some(field) = error.field().filter(|field| {
                schema.is_some_and(|schema| schema.fields().iter().any(|known| &known.id == *field))
            }) {
                domain = with_public_locator(domain, "component-field-id", field.as_str());
            }
            domain
        }
        ComponentCodecError::MissingField { .. } => with_sensitive(
            with_public_identifier(diagnostic, "codec-error-kind", "missing-field"),
            "field-path",
        ),
        ComponentCodecError::InvalidField { .. } => with_secret(
            with_sensitive(
                with_public_identifier(diagnostic, "codec-error-kind", "invalid-field"),
                "field-path",
            ),
            "codec-detail",
        ),
        ComponentCodecError::InvalidAssetRef { .. } => with_secret(
            with_sensitive(
                with_sensitive(
                    with_public_identifier(diagnostic, "codec-error-kind", "invalid-asset-ref"),
                    "field-path",
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
            None,
        ),
    }
}

pub(crate) fn with_identity_error(
    diagnostic: Diagnostic,
    error: &IdentityDomainError,
) -> Diagnostic {
    match error {
        IdentityDomainError::RuntimeEntityIncarnationExhausted => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "runtime-entity-incarnation-exhausted",
        ),
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
        IdentityDomainError::ProjectionEpochNotExclusive => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "projection-epoch-not-exclusive",
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
        IdentityDomainError::PersistentLogicalRecordLimit { requested, maximum } => {
            with_public_u64(
                with_public_u64(
                    with_public_identifier(
                        diagnostic,
                        "identity-error-kind",
                        "persistent-logical-record-limit",
                    ),
                    "requested-records",
                    usize_to_u64(*requested),
                ),
                "maximum-records",
                usize_to_u64(*maximum),
            )
        }
        IdentityDomainError::PersistentIdentityDeleted { .. } => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "persistent-identity-deleted",
        ),
        IdentityDomainError::PersistentIdentityActive { .. } => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "persistent-identity-active",
        ),
        IdentityDomainError::PersistentIdentityMissing { .. } => with_public_identifier(
            diagnostic,
            "identity-error-kind",
            "persistent-identity-missing",
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

#[cfg(test)]
mod tests {
    use super::*;
    use nara_diagnostic::{DiagnosticFieldClass, DiagnosticValueRef};
    use nara_identity::{EntityReference, SceneEntityId};
    use nara_reflect::{
        ComponentDomainError, ComponentFieldId, ComponentFieldSchema, ComponentNumericDomain,
        ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueShape, EntityReferenceTraversalLimits, rewrite_declared_entity_references,
        validate_component_value,
    };

    fn assert_redacted(diagnostic: &Diagnostic, key: &str) {
        let field = diagnostic
            .fields()
            .iter()
            .find(|field| field.key().as_str() == key)
            .expect("diagnostic field should be retained");
        assert_eq!(field.class(), DiagnosticFieldClass::Sensitive);
        assert_eq!(field.value(), DiagnosticValueRef::Redacted);
    }

    fn assert_identifier(diagnostic: &Diagnostic, key: &str, expected: &str) {
        assert!(diagnostic.fields().iter().any(|field| {
            field.key().as_str() == key
                && field.class() == DiagnosticFieldClass::Public
                && field.value() == DiagnosticValueRef::Identifier(expected)
        }));
    }

    fn assert_canary_absent(diagnostic: &Diagnostic, canary: &str) {
        assert!(!format!("{diagnostic:?}").contains(canary));
        #[cfg(feature = "serde")]
        assert!(!serde_json::to_string(diagnostic).unwrap().contains(canary));
    }

    #[test]
    fn reference_rewrite_diagnostics_redact_dynamic_map_keys() {
        let canary = "qvM7k2p8Nx4J";
        assert!(PublicDiagnosticIdentifier::new(canary).is_ok());
        let schema = ComponentSchema::new(
            ComponentTypeId::new("nara.test.DiagnosticReferences"),
            "Diagnostic references",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("targets"),
            "Targets",
            ComponentFieldPath::from_fields(["targets"]),
            ComponentValueShape::map(ComponentValueShape::EntityRef),
        )
        .with_capability(ComponentCapability::EntityRef)]);
        let value = ComponentValue::map([(
            "targets",
            ComponentValue::map([(
                canary,
                ComponentValue::EntityReference(EntityReference::SceneLocal {
                    entity: SceneEntityId::new("missing").unwrap(),
                }),
            )]),
        )]);
        let failure = rewrite_declared_entity_references(
            &schema,
            &value,
            EntityReferenceTraversalLimits::default(),
            |_, _| Err::<EntityReference, ()>(()),
        )
        .unwrap_err();
        assert!(matches!(
            &failure,
            ComponentEntityReferenceRewriteError::Rewrite { path, .. }
                if path == &ComponentFieldPath::from_fields(["targets", canary])
        ));

        let diagnostic = with_entity_reference_rewrite_error(
            error(
                "scene.test-reference-rewrite",
                "Scene test reference rewrite failed",
            ),
            &failure,
            |_| "missing-source-entity",
        );

        assert_redacted(&diagnostic, "reference-field-path");
        assert_identifier(&diagnostic, "rewrite-error-kind", "missing-source-entity");
        assert!(diagnostic.fields().iter().any(|field| {
            field.key().as_str() == "reference-field-path-depth"
                && field.value() == DiagnosticValueRef::Unsigned(2)
        }));
        assert_canary_absent(&diagnostic, canary);
    }

    #[test]
    fn codec_diagnostics_redact_nested_map_keys_and_unproven_field_text() {
        let canary = "qvM7k2p8Nx4J";
        assert!(PublicDiagnosticIdentifier::new(canary).is_ok());
        let schema = ComponentSchema::new(
            ComponentTypeId::new("nara.test.DiagnosticValues"),
            "Diagnostic values",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("weights"),
            "Weights",
            ComponentFieldPath::from_fields(["weights"]),
            ComponentValueShape::map(
                ComponentValueShape::scalar(ComponentValueKind::U64)
                    .with_numeric_domain(ComponentNumericDomain::U8),
            ),
        )]);
        let value = ComponentValue::map([(
            "weights",
            ComponentValue::map([(canary, ComponentValue::U64(256))]),
        )]);
        let failure = validate_component_value(&schema, &value).unwrap_err();
        assert!(matches!(
            &failure,
            ComponentCodecError::InvalidField { field, .. } if field.contains(canary)
        ));

        for (failure, expected_kind) in [
            (failure, "invalid-field"),
            (ComponentCodecError::missing_field(canary), "missing-field"),
            (
                ComponentCodecError::invalid_asset_ref(canary, canary, canary),
                "invalid-asset-ref",
            ),
        ] {
            let diagnostic = with_codec_error(
                error("scene.test-codec-error", "Scene test codec failed"),
                &failure,
                Some(&schema),
            );

            assert_redacted(&diagnostic, "field-path");
            assert_identifier(&diagnostic, "codec-error-kind", expected_kind);
            assert_canary_absent(&diagnostic, canary);
        }

        let path_failure = value
            .get_path(&ComponentFieldPath::from_fields([
                "weights", canary, canary,
            ]))
            .unwrap_err();
        let diagnostic = with_component_field_path_error(
            error("scene.test-nested-path", "Scene test nested path failed"),
            &path_failure,
        );
        assert_redacted(&diagnostic, "error-field-path");
        assert_identifier(&diagnostic, "path-error-kind", "expected-map");
        assert_canary_absent(&diagnostic, canary);

        let path_failure = value
            .get_path(&ComponentFieldPath::from_fields([canary]))
            .unwrap_err();
        let diagnostic = with_component_field_path_error(
            error("scene.test-missing-path", "Scene test map key was missing"),
            &path_failure,
        );
        assert_redacted(&diagnostic, "error-field-path");
        assert_redacted(&diagnostic, "missing-field");
        assert_identifier(&diagnostic, "path-error-kind", "missing-field");
        assert_canary_absent(&diagnostic, canary);
    }

    #[test]
    fn domain_diagnostics_preserve_identity_and_publish_only_schema_admitted_root_fields() {
        let canary = "qvM7k2p8Nx4J";
        let schema = ComponentSchema::new(
            ComponentTypeId::new("nara.test.DiagnosticDomain"),
            "Diagnostic domain",
            ComponentSchemaVersion::ONE,
        )
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("duration"),
            "Duration",
            ComponentFieldPath::from_fields(["duration"]),
            ComponentValueKind::I64,
        )]);
        for code in ["demo.cooldown.negative", "demo.cooldown.disabled"] {
            let failure = ComponentCodecError::domain(
                ComponentDomainError::new(
                    diagnostic_code(code),
                    safe_summary("Cooldown cannot be used"),
                )
                .with_field(ComponentFieldId::new("duration")),
            );
            let original = || {
                with_public_locator(
                    error(
                        "scene.invalid-component-payload",
                        "Component payload is invalid",
                    ),
                    "component-id",
                    "demo.Cooldown",
                )
            };
            let diagnostic = with_codec_error(original(), &failure, Some(&schema));
            assert_eq!(diagnostic.code().as_str(), code);
            assert_eq!(diagnostic.summary().as_str(), "Cooldown cannot be used");
            assert_identifier(&diagnostic, "component-field-id", "duration");
            assert_identifier(&diagnostic, "component-id", "demo.Cooldown");
            assert_identifier(
                &diagnostic,
                "scene-context-code",
                "scene.invalid-component-payload",
            );
            let unproven = with_codec_error(original(), &failure, None);
            assert_eq!(unproven.code().as_str(), code);
            assert_eq!(unproven.summary(), diagnostic.summary());
            assert!(
                !unproven
                    .fields()
                    .iter()
                    .any(|field| field.key().as_str() == "component-field-id")
            );
        }

        let foreign = ComponentCodecError::domain(
            ComponentDomainError::new(
                diagnostic_code("demo.cooldown.foreign-field"),
                safe_summary("Cooldown failed a domain check"),
            )
            .with_field(ComponentFieldId::new(canary)),
        );
        let diagnostic = with_codec_error(
            error(
                "scene.invalid-component-payload",
                "Component payload is invalid",
            ),
            &foreign,
            Some(&schema),
        );
        assert!(
            !diagnostic
                .fields()
                .iter()
                .any(|field| field.key().as_str() == "component-field-id")
        );
        assert_canary_absent(&diagnostic, canary);
    }
}
