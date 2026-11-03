//! Schema-owned payload rewriting, preserving source absence and hidden contextual defaults.

use std::{borrow::Cow, mem::size_of};

use nara_identity::EntityReference;
use nara_reflect::{
    __private::{
        ComponentPropertyAdmissionError, ComponentValueAdmissionError,
        EffectiveValueAdmissionError, EntityReferencePlanError,
        admit_declared_entity_reference_plan_rewrite,
        effective_component_value_with_default_overrides_with_admission,
        plan_declared_entity_references_with_admission,
        plan_declared_value_entity_references_with_admission,
        resolve_component_property_schema_claim_with_admission,
        validate_component_property_value_with_admission, validate_component_value_with_admission,
    },
    ComponentCapability, ComponentEntityReferenceRewriteError, ComponentFieldSchema,
    ComponentPropertyPath, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueShape,
};

use super::{ExtractionError, ExtractionWork};
use crate::{
    AuthoringSchemaBasis, PrefabPropertyClaim, PrefabPropertyIntent, SceneComponentRecord,
};

pub(super) fn require_schema<'basis>(
    component: &ComponentTypeId,
    version: Option<ComponentSchemaVersion>,
    basis: &'basis AuthoringSchemaBasis,
    work: &mut ExtractionWork,
) -> Result<&'basis ComponentSchema, ExtractionError> {
    work.visit(1, component.as_str().len(), 0)?;
    let schema = basis
        .schema(component)
        .ok_or(ExtractionError::UnknownSchema)?;
    if version.is_some_and(|version| version != schema.version()) {
        return Err(ExtractionError::VersionMismatch);
    }
    if !basis.has_authoring_adapter(component) {
        return Err(ExtractionError::AdapterUnavailable);
    }
    if !schema.has_capability(ComponentCapability::Scene) {
        return Err(ExtractionError::InvalidSource);
    }
    Ok(schema)
}

/// The enclosing construction owns this unpublished record and drops it on any refusal. Neither
/// effective validation nor rewriting changes omission into an explicit default-equal value.
pub(super) fn rewrite_record(
    component: &ComponentTypeId,
    record: &mut SceneComponentRecord,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
    rewrite: &mut impl FnMut(
        &EntityReference,
        &mut ExtractionWork,
    ) -> Result<EntityReference, ExtractionError>,
) -> Result<(), ExtractionError> {
    if preserve_unavailable {
        work.visit(
            1,
            component
                .as_str()
                .len()
                .saturating_add(record.owner().map_or(0, |owner| owner.as_str().len())),
            0,
        )?;
        if crate::schema_authority::SchemaAuthority::Authoring(basis)
            .unavailable_reason(component, record)
            .is_some()
        {
            return Ok(());
        }
    }
    let schema = require_schema(component, Some(record.version), basis, work)?;
    if record
        .owner()
        .is_some_and(|owner| basis.component_schema_owner(component) != Some(owner))
    {
        return Err(ExtractionError::OwnerMismatch);
    }
    let effective = validated_record_value(schema, record, work)?;
    drop(effective);

    let plan = plan_declared_entity_references_with_admission::<ExtractionError, _>(
        schema,
        &record.value,
        work.traversal()?,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(|error| plan_error(error, work))?;
    let cost = work.value(&record.value)?;
    admit_declared_entity_reference_plan_rewrite(&plan, cost, |nodes, bytes, allocated| {
        work.visit(nodes, bytes, allocated.ok_or(ExtractionError::Capacity)?)
    })?;
    plan.rewrite_in_place(&mut record.value, |_, reference| rewrite(reference, work))
        .map_err(|error| reference_error(error, work))?;

    for (id, value) in &mut record.contextual_defaults {
        let field = contextual_field(schema, id, work)?;
        rewrite_payload(field, field.shape(), value, work, rewrite)?;
    }
    work.value(&record.value)?;
    for value in record.contextual_defaults.values() {
        work.value(value)?;
    }
    Ok(())
}

fn validated_record_value<'value>(
    schema: &'value ComponentSchema,
    record: &'value SceneComponentRecord,
    work: &mut ExtractionWork,
) -> Result<Cow<'value, ComponentValue>, ExtractionError> {
    let limits = work.materialization()?;
    crate::document::validate_contextual_defaults_with_admission(
        schema,
        record,
        limits,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(effective_error)?;
    let effective = effective_component_value_with_default_overrides_with_admission(
        schema,
        &record.value,
        &record.contextual_defaults,
        limits,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(effective_error)?;
    validate_component_value_with_admission(
        schema,
        effective.as_ref(),
        limits,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(|error| match error {
        ComponentValueAdmissionError::Admission(error) => error,
        ComponentValueAdmissionError::Value(error) => ExtractionError::Value(error),
    })?;
    Ok(effective)
}

pub(super) fn visit_record_references(
    component: &ComponentTypeId,
    record: &SceneComponentRecord,
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
    visit: &mut impl FnMut(&EntityReference, &mut ExtractionWork) -> Result<(), ExtractionError>,
) -> Result<(), ExtractionError> {
    if preserve_unavailable
        && crate::schema_authority::SchemaAuthority::Authoring(basis)
            .unavailable_reason(component, record)
            .is_some()
    {
        return Ok(());
    }
    let schema = require_schema(component, Some(record.version), basis, work)?;
    if record
        .owner()
        .is_some_and(|owner| basis.component_schema_owner(component) != Some(owner))
    {
        return Err(ExtractionError::OwnerMismatch);
    }
    let effective = validated_record_value(schema, record, work)?;
    let plan = plan_declared_entity_references_with_admission::<ExtractionError, _>(
        schema,
        effective.as_ref(),
        work.traversal()?,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(|error| plan_error(error, work))?;
    plan.visit(effective.as_ref(), |_, reference| visit(reference, work))
        .map_err(|error| reference_error(error, work))?;
    // Stored defaults remain physical declarations even when a present explicit value masks
    // them. Later Absent/Revert can expose them, so their routes need the same continuity proof.
    for (id, value) in &record.contextual_defaults {
        let field = contextual_field(schema, id, work)?;
        let plan = plan_declared_value_entity_references_with_admission::<ExtractionError, _>(
            field,
            field.shape(),
            value,
            work.traversal()?,
            |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
        )
        .map_err(|error| plan_error(error, work))?;
        plan.visit(value, |_, reference| visit(reference, work))
            .map_err(|error| reference_error(error, work))?;
    }
    Ok(())
}

fn contextual_field<'schema>(
    schema: &'schema ComponentSchema,
    id: &nara_reflect::ComponentFieldId,
    work: &mut ExtractionWork,
) -> Result<&'schema ComponentFieldSchema, ExtractionError> {
    for field in schema.fields() {
        work.visit(
            1,
            field.id().as_str().len().saturating_add(id.as_str().len()),
            0,
        )?;
        if field.id() == id {
            return Ok(field);
        }
    }
    Err(ExtractionError::InvalidSource)
}

pub(super) fn visit_property_references(
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    claims: &[PrefabPropertyClaim],
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
    visit: &mut impl FnMut(&EntityReference, &mut ExtractionWork) -> Result<(), ExtractionError>,
) -> Result<(), ExtractionError> {
    let schema = match require_schema(component, Some(version), basis, work) {
        Ok(schema) => schema,
        Err(error) if preserve_unavailable && unavailable_schema_error(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    for claim in claims {
        let mut reads = work.property_reads;
        let selected = resolve_component_property_schema_claim_with_admission(
            schema,
            version,
            &claim.path,
            &mut reads,
            |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
        );
        work.property_reads = reads;
        let selected = selected.map_err(property_error)?;
        if let PrefabPropertyIntent::Set(value) = &claim.intent {
            validate_component_property_value_with_admission(
                selected.shape,
                &claim.path,
                value,
                work.materialization()?,
                |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
            )
            .map_err(property_error)?;
            let plan = plan_declared_value_entity_references_with_admission::<ExtractionError, _>(
                selected.field,
                selected.shape,
                value,
                work.traversal()?,
                |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
            )
            .map_err(|error| plan_error(error, work))?;
            plan.visit(value, |_, reference| visit(reference, work))
                .map_err(|error| reference_error(error, work))?;
        }
    }
    Ok(())
}

pub(super) fn rewrite_properties(
    component: &ComponentTypeId,
    version: ComponentSchemaVersion,
    claims: &mut [PrefabPropertyClaim],
    basis: &AuthoringSchemaBasis,
    preserve_unavailable: bool,
    work: &mut ExtractionWork,
    rewrite: &mut impl FnMut(
        &EntityReference,
        &mut ExtractionWork,
    ) -> Result<EntityReference, ExtractionError>,
) -> Result<(), ExtractionError> {
    let schema = match require_schema(component, Some(version), basis, work) {
        Ok(schema) => schema,
        Err(error) if preserve_unavailable && unavailable_schema_error(&error) => return Ok(()),
        Err(error) => return Err(error),
    };
    for claim in claims {
        let mut reads = work.property_reads;
        let selected = resolve_component_property_schema_claim_with_admission(
            schema,
            version,
            &claim.path,
            &mut reads,
            |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
        );
        work.property_reads = reads;
        let selected = selected.map_err(property_error)?;
        if let PrefabPropertyIntent::Set(value) = &mut claim.intent {
            rewrite_payload(selected.field, selected.shape, value, work, rewrite)?;
        }
        // Absent exposes the selected record's own contextual default. There is no payload
        // in this declaration to reinterpret in the overlay's lexical environment.
    }
    Ok(())
}

pub(super) fn unavailable_schema_error(error: &ExtractionError) -> bool {
    matches!(
        error,
        ExtractionError::UnknownSchema
            | ExtractionError::AdapterUnavailable
            | ExtractionError::VersionMismatch
            | ExtractionError::OwnerMismatch
    )
}

fn rewrite_payload(
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    value: &mut ComponentValue,
    work: &mut ExtractionWork,
    rewrite: &mut impl FnMut(
        &EntityReference,
        &mut ExtractionWork,
    ) -> Result<EntityReference, ExtractionError>,
) -> Result<(), ExtractionError> {
    work.admit(size_of::<ComponentPropertyPath>().checked_add(field.id().as_str().len()))?;
    let path = ComponentPropertyPath::new(field.id().clone());
    validate_component_property_value_with_admission(
        shape,
        &path,
        value,
        work.materialization()?,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(property_error)?;
    let plan = plan_declared_value_entity_references_with_admission::<ExtractionError, _>(
        field,
        shape,
        value,
        work.traversal()?,
        |nodes, bytes, allocated| work.visit(nodes, bytes, allocated),
    )
    .map_err(|error| plan_error(error, work))?;
    let cost = work.value(value)?;
    admit_declared_entity_reference_plan_rewrite(&plan, cost, |nodes, bytes, allocated| {
        work.visit(nodes, bytes, allocated.ok_or(ExtractionError::Capacity)?)
    })?;
    plan.rewrite_in_place(value, |_, reference| rewrite(reference, work))
        .map_err(|error| reference_error(error, work))?;
    work.value(value)?;
    Ok(())
}

fn effective_error(error: EffectiveValueAdmissionError<ExtractionError>) -> ExtractionError {
    match error {
        EffectiveValueAdmissionError::Admission(error) => error,
        EffectiveValueAdmissionError::Value(error) => ExtractionError::Value(error),
    }
}

fn property_error(error: ComponentPropertyAdmissionError<ExtractionError>) -> ExtractionError {
    match error {
        ComponentPropertyAdmissionError::Admission(error) => error,
        ComponentPropertyAdmissionError::Property(error) => ExtractionError::Property(error),
    }
}

fn plan_error(
    error: EntityReferencePlanError<ExtractionError, ExtractionError>,
    work: &mut ExtractionWork,
) -> ExtractionError {
    match error {
        EntityReferencePlanError::Admission(error) => error,
        EntityReferencePlanError::Reference(error) => reference_error(error, work),
    }
}

fn reference_error(
    error: ComponentEntityReferenceRewriteError<ExtractionError>,
    work: &mut ExtractionWork,
) -> ExtractionError {
    match error {
        ComponentEntityReferenceRewriteError::Rewrite { error, .. } => error,
        ComponentEntityReferenceRewriteError::NodeLimit { .. }
        | ComponentEntityReferenceRewriteError::ByteLimit { .. }
        | ComponentEntityReferenceRewriteError::DepthLimit { .. }
        | ComponentEntityReferenceRewriteError::PathIndexOverflow => ExtractionError::Capacity,
        error => match work.admit(Some(size_of::<
            ComponentEntityReferenceRewriteError<ExtractionError>,
        >())) {
            Ok(()) => ExtractionError::Reference(Box::new(error)),
            Err(error) => error,
        },
    }
}
