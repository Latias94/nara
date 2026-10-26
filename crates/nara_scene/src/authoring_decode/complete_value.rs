//! Complete value validation for source classification.

use std::borrow::Cow;

use super::*;

pub(super) fn validate_complete_value<'value>(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    value: &'value ComponentValue,
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
    let effective =
        effective_component_value(schema, value, limits.component_materialization_limits())
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
