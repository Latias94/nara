//! Lexical payload references in preserved external declarations, including dormant intent.

use super::*;
use crate::{PrefabComponentIntent, PrefabInstance, PrefabPropertyIntent};
use nara_reflect::{ComponentFieldSchema, ComponentValueShape};

pub(in crate::authoring::prefab_conversion::localize) fn localize_instance<'a>(
    original: &'a PrefabInstance,
    basis: &AuthoringSchemaBasis,
    targets: SourceTargets<'_>,
    limits: EntityReferenceTraversalLimits,
    work: &mut LocalizationWork<'_>,
) -> Result<Cow<'a, PrefabInstance>, ScenePatchReport> {
    prefab_instance_cost(original, instance_limits(work.maximum)).map_err(|_| copy_limit())?;
    let mut reads = nara_reflect::ComponentPropertyReadBudget::new(limits.nodes(), limits.bytes());
    rewrite_instance(original, basis, targets, limits, &mut reads, work)
}

fn rewrite_instance<'a>(
    original: &'a PrefabInstance,
    basis: &AuthoringSchemaBasis,
    targets: SourceTargets<'_>,
    limits: EntityReferenceTraversalLimits,
    reads: &mut nara_reflect::ComponentPropertyReadBudget,
    work: &mut LocalizationWork<'_>,
) -> Result<Cow<'a, PrefabInstance>, ScenePatchReport> {
    let mut next = Cow::Borrowed(original);
    if let Some(arguments) = &original.arguments {
        for (input, reference) in &arguments.values {
            if let Some(value) = replacement(reference, targets, work)? {
                instance_mut(&mut next, work)?
                    .arguments
                    .as_mut()
                    .expect("the original arguments are retained")
                    .values
                    .insert(input.clone(), value);
            }
        }
    }
    let Some(overlay) = &original.overrides else {
        return Ok(next);
    };
    for (index, addition) in overlay.additions.iter().enumerate() {
        for (component, original) in &addition.components {
            if let Cow::Owned(record) =
                localize_record(original, component, basis, targets, limits, work)?
            {
                instance_mut(&mut next, work)?
                    .overrides
                    .as_mut()
                    .expect("the original overlay is retained")
                    .additions[index]
                    .components
                    .insert(component.clone(), record);
            }
        }
        if let Some(original) = &addition.prefab
            && let Cow::Owned(instance) =
                rewrite_instance(original, basis, targets, limits, reads, work)?
        {
            instance_mut(&mut next, work)?
                .overrides
                .as_mut()
                .expect("the original overlay is retained")
                .additions[index]
                .prefab = Some(instance);
        }
    }
    for (index, claim) in overlay.instances.iter().enumerate() {
        if let Some(original) = &claim.value
            && let Cow::Owned(instance) =
                rewrite_instance(original, basis, targets, limits, reads, work)?
        {
            instance_mut(&mut next, work)?
                .overrides
                .as_mut()
                .expect("the original overlay is retained")
                .instances[index]
                .value = Some(instance);
        }
    }
    for (index, intent) in overlay.components.iter().enumerate() {
        match intent {
            PrefabComponentIntent::Set {
                component, record, ..
            } => {
                if let Cow::Owned(value) =
                    localize_record(record, component, basis, targets, limits, work)?
                {
                    let PrefabComponentIntent::Set { record, .. } =
                        &mut instance_mut(&mut next, work)?
                            .overrides
                            .as_mut()
                            .expect("the original overlay is retained")
                            .components[index]
                    else {
                        unreachable!("the source intent kind is unchanged");
                    };
                    *record = value;
                }
            }
            PrefabComponentIntent::Properties {
                component,
                version,
                claims,
                ..
            } => {
                let Some(schema) = basis.schema(component).filter(|schema| {
                    schema.version() == *version
                        && basis.has_authoring_adapter(component)
                        && schema.has_capability(nara_reflect::ComponentCapability::Scene)
                }) else {
                    continue;
                };
                for (claim_index, claim) in claims.iter().enumerate() {
                    let PrefabPropertyIntent::Set(value) = &claim.intent else {
                        continue;
                    };
                    let selected = nara_reflect::__private::resolve_component_property_schema_claim_with_admission(
                        schema, *version, &claim.path, reads,
                        |_, _, bytes| work.reserve(bytes),
                    );
                    let selected = match selected {
                        Ok(selected) => selected,
                        Err(
                            nara_reflect::__private::ComponentPropertyAdmissionError::Property(_),
                        ) => continue,
                        Err(
                            nara_reflect::__private::ComponentPropertyAdmissionError::Admission(
                                report,
                            ),
                        ) => return Err(report),
                    };
                    if let Cow::Owned(value) = localize_payload(
                        value,
                        selected.field,
                        selected.shape,
                        targets,
                        limits,
                        work,
                    )? {
                        let PrefabComponentIntent::Properties { claims, .. } =
                            &mut instance_mut(&mut next, work)?
                                .overrides
                                .as_mut()
                                .expect("the original overlay is retained")
                                .components[index]
                        else {
                            unreachable!("the source intent kind is unchanged");
                        };
                        claims[claim_index].intent = PrefabPropertyIntent::Set(value);
                    }
                }
            }
            PrefabComponentIntent::Suppress { .. } => {}
        }
    }
    Ok(next)
}

fn instance_limits(maximum: usize) -> PrefabOverlayCostLimits {
    PrefabOverlayCostLimits {
        nodes: ScenePatchApplyLimits::default().validation_work().get(),
        depth: SceneFileLimits::default().shape().depth().get(),
        logical_bytes: maximum,
        ..PrefabOverlayCostLimits::UNBOUNDED
    }
}

fn instance_mut<'value, 'original>(
    instance: &'value mut Cow<'original, PrefabInstance>,
    work: &mut LocalizationWork<'_>,
) -> Result<&'value mut PrefabInstance, ScenePatchReport> {
    if let Cow::Borrowed(original) = instance {
        let cost = prefab_instance_cost(original, instance_limits(work.maximum))
            .map_err(|_| copy_limit())?;
        work.reserve(
            cost.nodes
                .checked_mul(512)
                .and_then(|bytes| bytes.checked_add(cost.value_cost.nodes().checked_mul(96)?))
                .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
                .and_then(|bytes| bytes.checked_mul(2))
                .ok_or_else(copy_limit)?,
        )?;
    }
    Ok(instance.to_mut())
}

fn localize_payload<'a>(
    value: &'a ComponentValue,
    field: &ComponentFieldSchema,
    shape: &ComponentValueShape,
    targets: SourceTargets<'_>,
    limits: EntityReferenceTraversalLimits,
    work: &mut LocalizationWork<'_>,
) -> Result<Cow<'a, ComponentValue>, ScenePatchReport> {
    let plan = plan_declared_value_entity_references_with_admission::<ScenePatchReport, _>(
        field,
        shape,
        value,
        limits,
        |_, _, bytes| work.reserve(bytes),
    )
    .map_err(plan_error)?;
    let mut changed = false;
    plan.visit(value, |_, reference| {
        changed |= replacement(reference, targets, work)?.is_some();
        Ok::<_, ScenePatchReport>(())
    })
    .map_err(reference_error)?;
    if !changed {
        return Ok(Cow::Borrowed(value));
    }
    let cost = value.cost();
    work.reserve(
        cost.nodes()
            .checked_mul(96)
            .and_then(|bytes| bytes.checked_add(cost.logical_bytes()))
            .ok_or_else(copy_limit)?,
    )?;
    let mut next = value.clone();
    plan.rewrite_in_place(&mut next, |_, reference| {
        Ok::<_, ScenePatchReport>(
            replacement(reference, targets, work)?.unwrap_or_else(|| reference.clone()),
        )
    })
    .map_err(reference_error)?;
    Ok(Cow::Owned(next))
}
