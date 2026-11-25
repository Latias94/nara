//! Complete value validation for source classification.

use std::borrow::Cow;

use super::*;

pub(super) struct CompleteAuthoringValue<'a> {
    pub(super) effective: Cow<'a, ComponentValue>,
    pub(super) property_reads: Option<Arc<ComponentPropertyReadIndex>>,
}

pub(super) fn validate_complete_value<'value>(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    record: &'value SceneComponentRecord,
    limits: SceneFileLimits,
    admission: &mut Option<&mut reclassification::ClassificationCallback<'_>>,
) -> Result<CompleteAuthoringValue<'value>, AuthoringDocumentPublicationError> {
    let effective = if admission.is_some() {
        reclassification::effective_value(schema, record, limits, admission)?
    } else {
        check_contextual_record(schema, record, limits, admission)
            .map_err(|error| contextual_record_rejection(schema, error))?;
        nara_reflect::effective::effective_component_value_with_default_overrides_with_budget(
            schema,
            record.value(),
            &record.contextual_defaults,
            limits.component_value_nodes().get(),
            limits.component_value_bytes().get(),
            limits.shape().depth(),
        )
        .map(|(value, _)| value)
        .map_err(|error| {
            invalid_value(
                schema,
                owner,
                component,
                "Persistent authoring record exceeds its effective-value policy",
                &error,
            )
        })?
    };
    let property_reads =
        validate_property_read_index(schema, owner, component, record, &effective, admission)?;
    Ok(CompleteAuthoringValue {
        effective,
        property_reads,
    })
}

pub(super) fn validate_property_read_index(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    value: &ComponentValue,
    admission: &mut Option<&mut reclassification::ClassificationCallback<'_>>,
) -> Result<Option<Arc<ComponentPropertyReadIndex>>, AuthoringDocumentPublicationError> {
    use nara_reflect::registry::{
        ComponentValueAdmissionError, index_component_property_reads,
        index_component_property_reads_with_admission,
    };

    let invalid = |error| {
        invalid_value(
            schema,
            owner,
            component,
            "Persistent authoring record does not satisfy its declared schema",
            &error,
        )
    };
    let select = |field: &nara_reflect::ComponentFieldSchema| {
        let Some(_) = field.default_value() else {
            return true;
        };
        field.shape().has_keyed_property_lists()
            && (record.contextual_defaults.contains_key(field.id())
                || field
                    .path()
                    .segments()
                    .iter()
                    .try_fold(
                        nara_reflect::ComponentValueRef::from(record.value()),
                        |value, segment| match segment {
                            ComponentFieldPathSegment::Field(name) => value.get(name),
                            ComponentFieldPathSegment::Index(index) => {
                                value.as_list()?.get(*index as usize)
                            }
                        },
                    )
                    .is_some())
    };
    let index = if admission.is_some() {
        index_component_property_reads_with_admission(
            schema,
            value.into(),
            select,
            |nodes, bytes, allocated| {
                reclassification::reference_charge(
                    admission,
                    "classification-value-validation",
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(|error| match error {
            ComponentValueAdmissionError::Admission(report) => report,
            ComponentValueAdmissionError::Value(error) => invalid(error),
        })?
    } else {
        index_component_property_reads(schema, value.into(), select).map_err(invalid)?
    };
    Ok((!index.is_empty()).then(|| Arc::new(index)))
}

fn invalid_value(
    schema: &ComponentSchema,
    owner: &PersistentComponentSchemaOwnerId,
    component: &ComponentTypeId,
    summary: &'static str,
    error: &nara_reflect::ComponentCodecError,
) -> AuthoringDocumentPublicationError {
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
        error,
        Some(schema),
    ))
}

pub(super) enum ContextualRecordError {
    Value(nara_reflect::ComponentCodecError),
    Reference(nara_reflect::ComponentEntityReferenceRewriteError<()>),
    Admission(AuthoringDocumentPublicationError),
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
        ContextualRecordError::Admission(error) => return error,
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
    admission: &mut Option<&mut reclassification::ClassificationCallback<'_>>,
) -> Result<(), ContextualRecordError> {
    crate::document::validate_contextual_defaults_with_admission(
        schema,
        record,
        limits.component_materialization_limits(),
        |nodes, bytes, allocated| {
            reclassification::reference_charge(
                admission,
                "classification-contextual-validation",
                nodes,
                bytes,
                allocated,
            )
        },
    )
    .map_err(|error| match error {
        nara_reflect::effective::EffectiveValueAdmissionError::Value(error) => {
            ContextualRecordError::Value(error)
        }
        nara_reflect::effective::EffectiveValueAdmissionError::Admission(error) => {
            ContextualRecordError::Admission(error)
        }
    })?;
    for (id, value) in &record.contextual_defaults {
        let mut selected = None;
        for field in schema.fields() {
            reclassification::reference_charge(
                admission,
                "classification-contextual-reference-field",
                1,
                field.id().as_str().len().saturating_add(id.as_str().len()),
                0,
            )
            .map_err(ContextualRecordError::Admission)?;
            if field.id() == id {
                selected = Some(field);
                break;
            }
        }
        let field = selected.expect("contextual defaults retain admitted root IDs");
        nara_reflect::entity_reference::plan_declared_value_entity_references_with_admission::<(), _>(
            field,
            field.shape(),
            value,
            limits
                .component_materialization_limits()
                .reference_traversal_limits(),
            |nodes, bytes, allocated| {
                reclassification::reference_charge(
                    admission,
                    "classification-contextual-references",
                    nodes,
                    bytes,
                    allocated,
                )
            },
        )
        .map_err(|error| match error {
            nara_reflect::entity_reference::EntityReferencePlanError::Reference(error) => {
                ContextualRecordError::Reference(error)
            }
            nara_reflect::entity_reference::EntityReferencePlanError::Admission(error) => {
                ContextualRecordError::Admission(error)
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod contextual_admission_tests;
