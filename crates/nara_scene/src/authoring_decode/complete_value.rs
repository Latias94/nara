//! Complete value validation for source classification.

use std::borrow::Cow;

use super::*;

pub(super) fn validate_complete_value<'value>(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    record: &'value SceneComponentRecord,
    limits: SceneFileLimits,
) -> Result<Cow<'value, ComponentValue>, AuthoringDocumentPublicationError> {
    let invalid = |summary, error| {
        rejection(with_codec_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error("scene.authoring-component-value-invalid", summary),
                    "schema-owner",
                    owner.as_str(),
                ),
                "component-id",
                component.as_str(),
            ),
            &error,
            Some(schema),
        ))
    };
    validate_contextual_record(schema, record, limits)?;
    let effective =
        nara_reflect::__private::effective_component_value_with_default_overrides_with_budget(
            schema,
            &record.value,
            &record.contextual_defaults,
            limits.component_value_nodes().get(),
            limits.component_value_bytes().get(),
            limits.shape().depth(),
        )
        .map(|(value, _)| value)
        .map_err(|error| {
            invalid(
                "Persistent authoring record exceeds its effective-value policy",
                error,
            )
        })?;
    nara_reflect::validate_component_value(schema, &effective).map_err(|error| {
        invalid(
            "Persistent authoring record does not satisfy its declared schema",
            error,
        )
    })?;
    Ok(effective)
}

pub(super) fn validate_contextual_record(
    schema: &ComponentSchema,
    record: &SceneComponentRecord,
    limits: SceneFileLimits,
) -> Result<(), AuthoringDocumentPublicationError> {
    check_contextual_record(schema, record, limits)
        .map_err(|error| contextual_record_rejection(schema, error))
}

pub(super) enum ContextualRecordError {
    Value(nara_reflect::ComponentCodecError),
    Reference(nara_reflect::ComponentEntityReferenceRewriteError<()>),
}

pub(super) fn contextual_record_rejection(
    schema: &ComponentSchema,
    error: ContextualRecordError,
) -> AuthoringDocumentPublicationError {
    let error = match error {
        ContextualRecordError::Value(error) => error,
        ContextualRecordError::Reference(_) => nara_reflect::ComponentCodecError::invalid_field(
            "contextual defaults",
            "declared reference grammar",
        ),
    };
    rejection(with_codec_error(
        with_public_locator(
            diagnostic_error(
                "scene.authoring-contextual-default-invalid",
                "Contextual defaults must retain their complete current-version field contract",
            ),
            "component-id",
            schema.id().as_str(),
        ),
        &error,
        Some(schema),
    ))
}

/// Keep malformed declarations distinct from work limits until the source owner chooses
/// strict direct-record admission or exact dormant-overlay preservation.
pub(super) fn check_contextual_record(
    schema: &ComponentSchema,
    record: &SceneComponentRecord,
    limits: SceneFileLimits,
) -> Result<(), ContextualRecordError> {
    crate::document::validate_contextual_defaults(
        schema,
        record,
        limits.component_materialization_limits(),
    )
    .map_err(ContextualRecordError::Value)?;
    for (id, value) in &record.contextual_defaults {
        let field = schema
            .fields()
            .iter()
            .find(|field| field.id() == id)
            .expect("contextual defaults retain admitted root IDs");
        nara_reflect::__private::plan_declared_value_entity_references::<()>(
            field,
            field.shape(),
            value,
            reference_traversal_limits(limits.component_materialization_limits()),
        )
        .map_err(ContextualRecordError::Reference)?;
    }
    Ok(())
}
